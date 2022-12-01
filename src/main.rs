use std::{mem, thread};

use crossbeam_channel::{unbounded, Receiver, Sender};
use peek_poke::{
    ensure_red_zone, peek_from_slice, poke_extend_vec, poke_inplace_slice, poke_into_vec, PeekPoke,
    Poke,
};

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
    List,
    ListItem,
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

    fn push_list<I>(&mut self, list: I)
    where
        I: IntoIterator,
        I::IntoIter: ExactSizeIterator,
        I::Item: Poke,
    {
        self.push_item(&DisplayListItem::List);
        self.push_iter(list);
    }

    fn push_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator,
        I::IntoIter: ExactSizeIterator,
        I::Item: Poke,
    {
        let iter = iter.into_iter();
        let len = iter.len();
        let byte_size_offset = self.payload.len();
        poke_into_vec(&0usize, &mut self.payload);
        poke_into_vec(&len, &mut self.payload);
        let count = poke_extend_vec(iter, &mut self.payload);
        debug_assert_eq!(len, count, "iterator.len() returned two different values");

        // Add red zone
        ensure_red_zone::<I::Item>(&mut self.payload);

        // Now write the actual byte_size
        let final_offset = self.payload.len();
        debug_assert!(
            final_offset >= (byte_size_offset + mem::size_of::<usize>()),
            "space was never allocated for this array's byte_size"
        );
        let byte_size = final_offset - byte_size_offset - mem::size_of::<usize>();
        poke_inplace_slice(&byte_size, &mut self.payload[byte_size_offset..]);
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

    fn next_payload_as_item<'b>(
        &'b mut self,
        item: DisplayListItem,
    ) -> (Option<DisplayListItem>, Option<&'a [u8]>) {
        if self.data.len() <= DisplayListItem::max_size() {
            return (None, None);
        }

        let mut item = item;
        self.data = peek_from_slice(&self.data, &mut item);

        if let DisplayListItem::List = item {
            let mut skip = 0usize;
            self.data = peek_from_slice(self.data, &mut skip);
            let (skip, rest) = self.data.split_at(skip);
            self.data = rest;
            return (Some(item), Some(skip));
        }

        (Some(item), None)
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
            match iter.next_payload_as_item(DisplayListItem::None) {
                (Some(item), skip) => {
                    println!("Get DisplayItem in backend thread: {:?}", item);

                    let mut data = match skip {
                        Some(v) => v,
                        None => continue,
                    };

                    let mut item = DisplayListItem::None;

                    // Get array size from `data`
                    let mut size = 0usize;
                    if !data.is_empty() {
                        data = peek_from_slice(data, &mut size);
                    }

                    loop {
                        if size == 0 {
                            break;
                        }
                        size -= 1;

                        data = peek_from_slice(data, &mut item);
                        println!("Get DisplayItem::List in backend thread: {:?}", item);
                    }
                }
                _ => break,
            };
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

    display_list.push_list([DisplayListItem::ListItem, DisplayListItem::ListItem]);

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
