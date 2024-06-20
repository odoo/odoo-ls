use core::time;
use crate::core::event::UpdateEvent;
use std::{collections::VecDeque, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread::JoinHandle};

use super::odoo::SyncOdoo;

#[derive(Debug)]
struct EventQueueInternal {
    delay: u128,
    queue: VecDeque<UpdateEvent>,
    panic_mode: bool,
    thread: Option<JoinHandle<()>>,
    tx_channel: Sender<()>, //channel is only used to wake up thread, not to transfer data
    can_continue: bool
}

#[derive(Debug)]
pub struct EventQueue {
    internal: Arc<Mutex<EventQueueInternal>>,
}

fn thread_fn(rx: std::sync::mpsc::Receiver<()>, internal: Arc<Mutex<EventQueueInternal>>) {
    let mut must_stop = false;
    let mut timeout: u128 = 0;
    while !must_stop && rx.recv_timeout(std::time::Duration::from_millis(timeout as u64)).is_ok() {
        let lock = internal.lock();
        if let Ok(mut lock) = lock {
            if !lock.can_continue {
                must_stop = true;
                break;
            }
            if lock.queue.len() > 0 {
                let elapsed = lock.queue.back().unwrap().get_time().elapsed();
                if elapsed.as_millis() < lock.delay {
                    timeout = lock.delay - elapsed.as_millis();
                    break;
                }
                if lock.panic_mode {
                    //TODO implement panic mode
                    lock.queue.clear();
                    lock.panic_mode = false;
                    break;
                }
                while let Some(mut event) = lock.queue.pop_front() {
                    event.process();
                }
            } else {
                timeout = 5000;
            }

        } else {
            must_stop = true;
        }
    }
}

impl EventQueue {

    pub fn new(delay: u128) -> Self {
        let (tx_channel, rx) = channel();
        let internal = Arc::new(Mutex::new(EventQueueInternal{
            delay: delay,
            queue: VecDeque::new(),
            panic_mode: false,
            thread: None,
            tx_channel: tx_channel,
            can_continue: false
        }));
        let internal_arc = internal.clone();
        internal.lock().unwrap().thread = Some(std::thread::spawn(move || thread_fn(rx, internal_arc)));
        Self {
            internal: internal
        }
    }

    pub fn add_event(&self, mut event: UpdateEvent) {
        let internal = self.internal.lock();
        if let Ok(mut internal) = internal {
            if internal.panic_mode {
                if internal.queue.len() > 0 {
                    internal.queue.back_mut().unwrap().set_time(std::time::Instant::now());
                    return;
                }
                else {
                    internal.panic_mode = false; //should never happen, but if it's the case, let's stop the panic mode.
                }
            }
            event.set_time(std::time::Instant::now());
            internal.queue.push_back(event);
            if internal.queue.len() > 20 {
                internal.panic_mode = true;
            }
            let _ = internal.tx_channel.send(()); //wake up thread
        }
    }

    pub fn set_delay(&self, delay: u128) {
        let internal = self.internal.lock();
        if let Ok(mut internal) = internal {
            internal.delay = delay;
            let _ = internal.tx_channel.send(()); //wake up thread
        }
    }
}