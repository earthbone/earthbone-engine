// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry,FileHeader,GlobalFormId,Group,GroupKind,LoadOrder,Plugin,Record,Tag};
use soe_game::Game;
fn id(n:u32)->GlobalFormId{GlobalFormId{file:0,object:n}}
fn topic(n:u32,quest:u32,branch:u32)->Record{Record::new(Tag(*b"DIAL"),n).zsub(Tag(*b"FULL"),"Prompt").sub(Tag(*b"QNAM"),quest.to_le_bytes()).sub(Tag(*b"BNAM"),branch.to_le_bytes()).sub(Tag(*b"DATA"),[0;4]).sub(Tag(*b"SNAM"),*b"CUST").sub(Tag(*b"PNAM"),50f32.to_le_bytes())}
fn info(n:u32,flags:u16,text:&str,link:Option<u32>)->Record{let mut r=Record::new(Tag(*b"INFO"),n).sub(Tag(*b"ENAM"),[flags as u8,(flags>>8)as u8,0,0]);let mut response=vec![0;24];response[12]=1;r=r.sub(Tag(*b"TRDT"),response).zsub(Tag(*b"NAM1"),text);if let Some(link)=link{r=r.sub(Tag(*b"TCLT"),link.to_le_bytes())}r}
#[test]
fn dlg_15_invisible_continue_follows_only_authored_link_without_replaying_current_line(){
 let mut p=Plugin::new(FileHeader::default());for(f,name)in[(7,"Player"),(0x100,"Speaker")]{p.push_top(Record::new(Tag(*b"NPC_"),f).zsub(Tag(*b"EDID"),name).sub(Tag(*b"ACBS"),vec![0;24]));}
 p.push_top(Record::new(Tag(*b"ACHR"),0x200).sub(Tag(*b"NAME"),0x100u32.to_le_bytes()).sub(Tag(*b"DATA"),vec![0;24]));p.push_top(Record::new(Tag(*b"QUST"),0x300).sub(Tag(*b"DNAM"),vec![0;12]).sub(Tag(*b"NEXT"),[]).sub(Tag(*b"INDX"),[0;4]).sub(Tag(*b"INDX"),5u32.to_le_bytes()).sub(Tag(*b"INDX"),10u32.to_le_bytes()).sub(Tag(*b"INDX"),20u32.to_le_bytes()).sub(Tag(*b"INDX"),5u32.to_le_bytes()).sub(Tag(*b"INDX"),10u32.to_le_bytes()).sub(Tag(*b"INDX"),20u32.to_le_bytes()));p.push_top(Record::new(Tag(*b"DLBR"),0x1400).sub(Tag(*b"QNAM"),0x300u32.to_le_bytes()).sub(Tag(*b"TNAM"),0u32.to_le_bytes()).sub(Tag(*b"DNAM"),1u32.to_le_bytes()).sub(Tag(*b"SNAM"),0x400u32.to_le_bytes()));
 let first=topic(0x400,0x300,0x1400);let linked=topic(0x401,0x300,0x1400);let mut root=Group::new(GroupKind::Top(Tag(*b"DIAL")));root.entries.push(Entry::Record(p.push_record(first)));let mut c=Group::new(GroupKind::TopicChildren(0x400));c.entries.push(Entry::Record(p.push_record(info(0x500,1<<6,"Hidden",Some(0x401)))));root.entries.push(Entry::Group(c));root.entries.push(Entry::Record(p.push_record(linked)));let mut c2=Group::new(GroupKind::TopicChildren(0x401));c2.entries.push(Entry::Record(p.push_record(info(0x501,0,"Next",None))));root.entries.push(Entry::Group(c2));p.groups.push(root);
 let mut lo=LoadOrder::new();lo.add("Dialogue.esm",p).unwrap();let mut g=Game::new(lo);g.start_quest(id(0x300));let player=g.player();g.begin_dialogue(id(0x200),player).unwrap();let first=g.choose_dialogue(id(0x400)).unwrap().unwrap();assert_eq!(first.info,id(0x500));let next=g.advance_dialogue().unwrap().unwrap();assert_eq!(next.info,id(0x501));assert_eq!(next.text.as_deref(),Some("Next"));
}
