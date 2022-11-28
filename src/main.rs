use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};
use peek_poke::{ensure_red_zone, peek_from_slice, poke_into_vec, PeekPoke, Poke};

enum Message {
    SetDisplayList(DisplayList),
    Close,
}

#[derive(PeekPoke, Default, Debug)]
struct RectItem {
    min: (f32, f32),
    max: (f32, f32),
}

#[derive(PeekPoke, Debug)]
enum DisplayListItem {
    Rect(RectItem),
    None,
}

struct DisplayList {
    payload: Vec<u8>,
}

impl DisplayList {
    fn new() -> Self {
        DisplayList {
            payload: Vec::new(),
        }
    }

    fn push_item(&mut self, item: &DisplayListItem) {
        poke_into_vec(item, &mut self.payload);
        println!("Set DisplayItem in main thread: {:?}", item);
    }

    fn end(&mut self) {
        ensure_red_zone::<DisplayListItem>(&mut self.payload);
    }

    fn iter(&self) -> DisplayListIter {
        DisplayListIter::new(&self.payload)
    }
}

struct DisplayListIter<'a> {
    data: &'a [u8],
}

impl<'a> DisplayListIter<'a> {
    fn new(data: &'a [u8]) -> Self {
        DisplayListIter { data }
    }

    fn next_payload_as_item<'b>(&'b mut self, item: DisplayListItem) -> Option<DisplayListItem> {
        if self.data.len() <= DisplayListItem::max_size() {
            return None;
        }

        let mut item = item;
        self.data = peek_from_slice(&self.data, &mut item);
        Some(item)
    }
}

struct Backend {
    receiver: Receiver<Message>,
    result_sender: Sender<()>,
}

impl Backend {
    fn new(receiver: Receiver<Message>, result_sender: Sender<()>) -> Self {
        Backend {
            receiver,
            result_sender,
        }
    }

    fn run(&self) {
        loop {
            match self.receiver.recv().expect("Could not receive Message") {
                Message::SetDisplayList(dl) => {
                    let iter = dl.iter();
                    self.process(iter);
                    self.result_sender.send(()).expect("Could not send result");
                }
                Message::Close => break,
            };
        }
    }

    fn process(&self, iter: DisplayListIter) {
        let mut iter = iter;
        loop {
            let item = match iter.next_payload_as_item(DisplayListItem::None) {
                Some(item) => item,
                None => break,
            };
            println!("Get DisplayItem in backend thread: {:?}", item);
        }
    }
}

fn main() {
    let (sender, receiver) = unbounded();
    let (result_sender, result_receiver) = unbounded();

    let backend_thread_name = "backend".to_owned();
    thread::Builder::new()
        .name(backend_thread_name)
        .spawn(move || {
            let b = Backend::new(receiver, result_sender);
            b.run();
        })
        .expect("Backend thread could not spawn");

    let mut display_list = DisplayList::new();
    display_list.push_item(&DisplayListItem::Rect(RectItem {
        min: (100., 100.),
        max: (500., 500.),
    }));
    display_list.push_item(&DisplayListItem::Rect(RectItem {
        min: (500., 500.),
        max: (1000., 1000.),
    }));
    display_list.end();

    sender
        .send(Message::SetDisplayList(display_list))
        .expect("Could not send display_list");
    println!("Send display_list");

    result_receiver.recv().expect("Could not receive result");

    sender
        .send(Message::Close)
        .expect("Could not send close message");
}
