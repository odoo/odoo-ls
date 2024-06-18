use core::time;
use std::{collections::VecDeque, sync::{Arc, Mutex}, thread::{JoinHandle, Thread}};

use super::odoo::Odoo;

enum UpdateEventType {
    CREATE,
    UPDATE,
    DELETE,
}

struct UpdateEvent {
    event_type: UpdateEventType,
    time: std::time::Instant,
}

struct EventQueueInternal {
    delay: u32,
    queue: VecDeque<UpdateEvent>,
    panic_mode: bool,
    thread: Option<JoinHandle<()>>,
}

struct EventQueue {
    odoo: Arc<Mutex<Odoo>>,
    internal: Arc<Mutex<EventQueueInternal>>,
}

fn thread_fn(internal: Arc<Mutex<EventQueueInternal>>) {
    std::thread::sleep(time::Duration::from_millis(1000));
}

impl EventQueue {


    pub fn add_event(&mut self, mut event: UpdateEvent) {
        let internal = self.internal.lock();
        if let Ok(mut internal) = internal {
            if internal.panic_mode {
                if internal.queue.len() > 0 {
                    internal.queue.back_mut().unwrap().time = std::time::Instant::now();
                    return;
                }
                else {
                    internal.panic_mode = false; //should never happen, but if it's the case, let's stop the panic mode.
                }
            }
            event.time = std::time::Instant::now();
            internal.queue.push_back(event);
            if internal.queue.len() > 20 {
                internal.panic_mode = true;
            }
            if internal.thread.is_none() || internal.thread.as_ref().unwrap().is_finished() {
                let internal_arc = self.internal.clone();
                internal.thread = Some(std::thread::spawn(move || thread_fn(internal_arc)));
            }
        }
    }
}