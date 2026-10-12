// SPDX-License-Identifier: GPL-3.0-or-later

//! Latest-request background decoding and a bounded PCM cache for AUD-12.
//! OpenMW's mwsound/soundmanagerimp.cpp keeps a pending music selection.
//! Its OpenAL stream backend does not fit the Godot host. Here a Rust worker
//! owns decoding and caching, and Godot's generator consumes the returned PCM.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use soe_audio::Pcm;

pub(crate) const CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Lane {
    Music,
    Ambience,
}

impl Lane {
    fn index(self) -> usize {
        match self {
            Self::Music => 0,
            Self::Ambience => 1,
        }
    }
}

enum Command {
    Decode(Request),
    Retire(Arc<Pcm>),
}

struct Request {
    lane: Lane,
    ticket: u64,
    path: String,
}

pub(crate) struct Ready {
    pub lane: Lane,
    pub ticket: u64,
    pub path: String,
    pub pcm: Result<Arc<Pcm>, String>,
}

#[derive(Default)]
struct Counters {
    latest: [AtomicU64; 2],
    decodes: AtomicU64,
    cache_bytes: AtomicUsize,
}

pub(crate) struct Decoder {
    requests: mpsc::Sender<Command>,
    ready: mpsc::Receiver<Ready>,
    counters: Arc<Counters>,
}

impl Decoder {
    pub fn new(
        decode: impl Fn(&str) -> Result<Pcm, String> + Send + 'static,
        cap: usize,
    ) -> Result<Self, String> {
        let (requests, incoming) = mpsc::channel::<Command>();
        let (outgoing, ready) = mpsc::channel();
        let counters = Arc::new(Counters::default());
        let worker_counters = Arc::clone(&counters);
        std::thread::Builder::new()
            .name("soe-audio-decode".into())
            .spawn(move || {
                let mut cache = Cache::new(cap);
                while let Ok(command) = incoming.recv() {
                    let request = match command {
                        Command::Decode(request) => request,
                        Command::Retire(pcm) => {
                            drop(pcm);
                            continue;
                        }
                    };
                    let latest = &worker_counters.latest[request.lane.index()];
                    if latest.load(Ordering::Acquire) != request.ticket {
                        continue;
                    }
                    let pcm = cache.get(&request.path).map_or_else(
                        || {
                            worker_counters.decodes.fetch_add(1, Ordering::Relaxed);
                            decode(&request.path).and_then(validate).map(Arc::new)
                        },
                        Ok,
                    );
                    // CONTEXT: superseded tracks never enter the cache or the
                    // main-thread completion queue, including decode failures.
                    if latest.load(Ordering::Acquire) != request.ticket {
                        continue;
                    }
                    if let Ok(pcm) = &pcm {
                        cache.insert(request.path.clone(), Arc::clone(pcm));
                        worker_counters
                            .cache_bytes
                            .store(cache.bytes, Ordering::Relaxed);
                    }
                    if outgoing
                        .send(Ready {
                            lane: request.lane,
                            ticket: request.ticket,
                            path: request.path,
                            pcm,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("audio worker: {error}"))?;
        Ok(Self {
            requests,
            ready,
            counters,
        })
    }

    pub fn request(&self, lane: Lane, path: &str) -> Result<u64, String> {
        let ticket = self.cancel(lane);
        self.requests
            .send(Command::Decode(Request {
                lane,
                ticket,
                path: soe_bsa::normalize(path),
            }))
            .map_err(|error| format!("audio worker stopped: {error}"))?;
        Ok(ticket)
    }

    pub fn cancel(&self, lane: Lane) -> u64 {
        self.counters.latest[lane.index()].fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn retire(&self, pcm: Arc<Pcm>) {
        // CONTEXT: uncached PCM can require a large unmap. Transfer ownership
        // so the last reference is released by the worker rather than a frame.
        let _ = self.requests.send(Command::Retire(pcm));
    }

    pub fn poll(&self) -> Option<Ready> {
        self.ready.try_recv().ok()
    }

    pub fn decodes(&self) -> u64 {
        self.counters.decodes.load(Ordering::Relaxed)
    }

    pub fn cache_bytes(&self) -> usize {
        self.counters.cache_bytes.load(Ordering::Relaxed)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // CONTEXT: never join a long decode on the host thread. Closing the
        // channels and invalidating both lanes makes the worker exit afterward.
        self.cancel(Lane::Music);
        self.cancel(Lane::Ambience);
        while let Some(ready) = self.poll() {
            if let Ok(pcm) = ready.pcm {
                self.retire(pcm);
            }
        }
    }
}

fn validate(pcm: Pcm) -> Result<Pcm, String> {
    if pcm.is_empty()
        || pcm.sample_rate == 0
        || !(1..=2).contains(&pcm.channels)
        || !pcm.samples.len().is_multiple_of(usize::from(pcm.channels))
        || pcm.samples.iter().any(|sample| !sample.is_finite())
    {
        return Err("background audio requires finite, nonempty mono or stereo PCM".into());
    }
    Ok(pcm)
}

struct Cache {
    entries: HashMap<String, Arc<Pcm>>,
    order: VecDeque<String>,
    bytes: usize,
    cap: usize,
}

impl Cache {
    fn new(cap: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            cap,
        }
    }

    fn get(&mut self, path: &str) -> Option<Arc<Pcm>> {
        let pcm = Arc::clone(self.entries.get(path)?);
        self.order.retain(|entry| entry != path);
        self.order.push_back(path.to_owned());
        Some(pcm)
    }

    fn insert(&mut self, path: String, pcm: Arc<Pcm>) {
        if self.entries.contains_key(&path) {
            return;
        }
        let bytes = pcm.samples.capacity().saturating_mul(size_of::<f32>());
        if bytes > self.cap {
            return;
        }
        while self.bytes.saturating_add(bytes) > self.cap {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(pcm) = self.entries.remove(&oldest) {
                self.bytes -= pcm.samples.capacity() * size_of::<f32>();
            }
        }
        self.bytes += bytes;
        self.order.push_back(path.clone());
        self.entries.insert(path, pcm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pcm(samples: usize) -> Pcm {
        Pcm {
            sample_rate: 22_050,
            channels: 1,
            samples: vec![0.5; samples],
        }
    }

    fn receive(decoder: &Decoder) -> Ready {
        decoder.ready.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn aud_12_music_and_ambience_decode_on_a_worker_and_cache_normalized_paths() {
        let caller = std::thread::current().id();
        let decoder = Decoder::new(
            move |_| {
                assert_ne!(std::thread::current().id(), caller);
                Ok(pcm(8))
            },
            64,
        )
        .unwrap();
        decoder.request(Lane::Music, "Music\\Track.WAV").unwrap();
        let first = receive(&decoder).pcm.unwrap();
        decoder.request(Lane::Ambience, "music/track.wav").unwrap();
        let second = receive(&decoder).pcm.unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(decoder.decodes(), 1);
        assert_eq!(decoder.cache_bytes(), 32);
    }

    #[test]
    fn aud_12_superseded_and_cancelled_decodes_are_dropped_before_caching() {
        let (started, start) = mpsc::channel();
        let (finish, finished) = mpsc::channel();
        let decoder = Decoder::new(
            move |path| {
                if path == "old" {
                    started.send(()).unwrap();
                    finished.recv().unwrap();
                }
                Ok(pcm(8))
            },
            64,
        )
        .unwrap();
        decoder.request(Lane::Music, "old").unwrap();
        start.recv_timeout(Duration::from_secs(5)).unwrap();
        decoder.request(Lane::Music, "middle").unwrap();
        decoder.request(Lane::Music, "new").unwrap();
        decoder.request(Lane::Ambience, "cancelled").unwrap();
        decoder.cancel(Lane::Ambience);
        finish.send(()).unwrap();
        assert_eq!(receive(&decoder).path, "new");
        assert!(decoder.poll().is_none());
        assert_eq!(decoder.decodes(), 2);
        assert_eq!(decoder.cache_bytes(), 32);
    }

    #[test]
    fn aud_12_cache_evicts_least_recent_tracks_and_bypasses_oversized_pcm() {
        let mut cache = Cache::new(64);
        let first = Arc::new(pcm(8));
        cache.insert("first".into(), Arc::clone(&first));
        cache.insert("second".into(), Arc::new(pcm(8)));
        cache.get("first");
        cache.insert("third".into(), Arc::new(pcm(8)));
        assert!(cache.get("second").is_none());
        assert!(Arc::ptr_eq(&first, &cache.get("first").unwrap()));
        cache.insert("huge".into(), Arc::new(pcm(17)));
        assert!(cache.get("huge").is_none());
        assert_eq!(cache.bytes, 64);
    }

    #[test]
    fn aud_12_uncached_pcm_is_retired_by_the_worker() {
        let buffer = Arc::new(pcm(8));
        let released = Arc::downgrade(&buffer);
        let caller = std::thread::current().id();
        let decoder = Decoder::new(
            move |_| {
                assert_ne!(std::thread::current().id(), caller);
                assert!(
                    released.upgrade().is_none(),
                    "retired PCM was released before the next worker task"
                );
                Ok(pcm(8))
            },
            0,
        )
        .unwrap();
        decoder.retire(buffer);
        decoder.request(Lane::Music, "next").unwrap();
        assert!(receive(&decoder).pcm.is_ok());
        assert_eq!(decoder.cache_bytes(), 0);
    }

    #[test]
    fn aud_12_decode_errors_and_invalid_pcm_are_reported_without_caching() {
        let decoder = Decoder::new(
            |path| {
                if path == "missing" {
                    Err("missing sound".into())
                } else {
                    Ok(pcm(0))
                }
            },
            64,
        )
        .unwrap();
        for path in ["missing", "empty"] {
            decoder.request(Lane::Music, path).unwrap();
            assert!(receive(&decoder).pcm.is_err());
        }
        assert_eq!(decoder.cache_bytes(), 0);
    }
}
