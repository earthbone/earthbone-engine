// SPDX-License-Identifier: GPL-3.0-or-later

//! One-shot audio preparation on a worker, following Godot's local thread-safe
//! APIs reference. Archive reads, codecs, lip parsing and mono PCM conversion
//! never touch Godot objects. The host publishes one ready stream per frame.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use soe_audio::{LipWeightTrack, Sound};

pub(crate) struct PreparedSound {
    pub bytes: Vec<u8>,
    pub rate: u32,
    pub duration: f64,
    pub lip: Option<LipWeightTrack>,
}

impl PreparedSound {
    pub fn prepare(sound: &Sound) -> Result<Self, String> {
        if sound.pcm.is_empty() || sound.pcm.sample_rate == 0 {
            return Err("decoded to no samples".into());
        }
        let duration = sound.pcm.duration_seconds();
        let lip = sound.lip.as_deref().and_then(|bytes| {
            soe_audio::Lip::parse(bytes)
                .and_then(|lip| lip.decode())
                .ok()
        });
        let mono = crate::audio::to_mono(&sound.pcm);
        Ok(Self {
            bytes: crate::audio::to_i16_bytes(&mono),
            rate: mono.sample_rate,
            duration,
            lip,
        })
    }

    fn size(&self) -> usize {
        self.bytes
            .capacity()
            .saturating_add(self.lip.as_ref().map_or(0, |lip| {
                lip.frame_count() as usize
                    * (lip.target_count() + lip.gesture_count())
                    * size_of::<f32>()
            }))
    }
}

pub(crate) struct Ready {
    pub slot: usize,
    pub ticket: u64,
    pub path: String,
    pub sound: Result<Arc<PreparedSound>, String>,
}

struct Request {
    slot: usize,
    ticket: u64,
    path: String,
}

pub(crate) struct OneShotDecoder {
    requests: mpsc::SyncSender<Request>,
    ready: mpsc::Receiver<Ready>,
    latest: Arc<Vec<AtomicU64>>,
    decodes: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl OneShotDecoder {
    pub fn new(
        slots: usize,
        cap: usize,
        decode: impl Fn(&str) -> Result<Sound, String> + Send + 'static,
    ) -> Result<Self, String> {
        let (requests, incoming) = mpsc::sync_channel::<Request>(slots);
        let (outgoing, ready) = mpsc::channel();
        let latest: Arc<Vec<AtomicU64>> = Arc::new((0..slots).map(|_| AtomicU64::new(0)).collect());
        let worker_latest = Arc::clone(&latest);
        let decodes = Arc::new(AtomicU64::new(0));
        let worker_decodes = Arc::clone(&decodes);
        let worker = std::thread::Builder::new()
            .name("soe-one-shot-audio".into())
            .spawn(move || {
                let mut cache = HashMap::<String, Arc<PreparedSound>>::new();
                let mut order = VecDeque::new();
                let mut bytes = 0_usize;
                while let Ok(request) = incoming.recv() {
                    let current = &worker_latest[request.slot];
                    if current.load(Ordering::Acquire) != request.ticket {
                        continue;
                    }
                    let sound = cache.get(&request.path).cloned().map_or_else(
                        || {
                            worker_decodes.fetch_add(1, Ordering::Relaxed);
                            let started = std::time::Instant::now();
                            let result = decode(&request.path)
                                .and_then(|sound| PreparedSound::prepare(&sound))
                                .map(Arc::new);
                            soe_trace::metric_add(
                                soe_trace::Metric::AudioDecodeNs,
                                started.elapsed().as_nanos() as u64,
                            );
                            result
                        },
                        Ok,
                    );
                    if current.load(Ordering::Acquire) != request.ticket {
                        continue;
                    }
                    if let Ok(sound) = &sound {
                        let size = sound.size();
                        if size <= cap && !cache.contains_key(&request.path) {
                            while bytes.saturating_add(size) > cap {
                                let Some(oldest) = order.pop_front() else {
                                    break;
                                };
                                if let Some(old) = cache.remove(&oldest) {
                                    bytes -= old.size();
                                }
                            }
                            bytes += size;
                            cache.insert(request.path.clone(), Arc::clone(sound));
                        }
                        if cache.contains_key(&request.path) {
                            order.retain(|path| path != &request.path);
                            order.push_back(request.path.clone());
                        }
                    }
                    if outgoing
                        .send(Ready {
                            slot: request.slot,
                            ticket: request.ticket,
                            path: request.path,
                            sound,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("one-shot audio worker: {error}"))?;
        Ok(Self {
            requests,
            ready,
            latest,
            decodes,
            worker: Some(worker),
        })
    }

    pub fn request(&self, slot: usize, path: &str) -> Result<u64, String> {
        let ticket = self.cancel(slot);
        self.requests
            .try_send(Request {
                slot,
                ticket,
                path: soe_bsa::normalize(path),
            })
            .map_err(|error| format!("audio request queue: {error}"))?;
        Ok(ticket)
    }

    pub fn cancel(&self, slot: usize) -> u64 {
        self.latest[slot].fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn poll(&self) -> Option<Ready> {
        self.ready.try_recv().ok()
    }

    pub fn decodes(&self) -> u64 {
        self.decodes.load(Ordering::Relaxed)
    }
}

impl Drop for OneShotDecoder {
    fn drop(&mut self) {
        for slot in 0..self.latest.len() {
            self.cancel(slot);
        }
        if let Some(worker) = self.worker.take() {
            crate::background_audio::retire_worker(worker);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sound() -> Sound {
        Sound {
            container: soe_audio::Container::Wave,
            pcm: soe_audio::Pcm {
                sample_rate: 22050,
                channels: 2,
                samples: vec![0.25; 64],
            },
            lip: None,
        }
    }

    #[test]
    fn r4_one_shot_decode_and_conversion_run_off_thread_and_reuse_normalized_paths() {
        let caller = std::thread::current().id();
        let decoder = OneShotDecoder::new(2, 1024, move |_| {
            assert_ne!(std::thread::current().id(), caller);
            Ok(sound())
        })
        .unwrap();
        decoder.request(0, "Sound\\Foot.WAV").unwrap();
        let first = decoder
            .ready
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .sound
            .unwrap();
        decoder.request(1, "sound/foot.wav").unwrap();
        let second = decoder
            .ready
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .sound
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.bytes.len(), 64);
        assert_eq!(decoder.decodes(), 1);
    }

    #[test]
    fn r4_blocked_one_shot_decoder_never_blocks_requests_or_starts_cancelled_slots() {
        let (started, start) = mpsc::channel();
        let (finish, finished) = mpsc::channel();
        let decoder = OneShotDecoder::new(2, 1024, move |path| {
            if path == "blocked" {
                started.send(()).unwrap();
                finished.recv().unwrap();
            }
            Ok(sound())
        })
        .unwrap();
        decoder.request(0, "blocked").unwrap();
        start.recv_timeout(Duration::from_secs(5)).unwrap();
        let started = std::time::Instant::now();
        decoder.cancel(0);
        let ticket = decoder.request(1, "latest").unwrap();
        assert!(started.elapsed() < Duration::from_millis(2));
        finish.send(()).unwrap();
        let ready = decoder.ready.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            (ready.slot, ready.ticket, ready.path.as_str()),
            (1, ticket, "latest")
        );
        assert!(decoder.poll().is_none());
    }
}
