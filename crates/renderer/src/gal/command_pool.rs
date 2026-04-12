use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use smallvec::SmallVec;

use crate::gal::command_pool_resource::CommandPoolResource;
use crate::gal::queue::Lane;
use crate::gal::usage_token::UsageToken;

pub struct CommandPool {
    resource: CommandPoolResource,
    lane: Lane,
    usage: Rc<RefCell<UsageToken>>,
}

impl CommandPool {
    pub(crate) fn new(resource: CommandPoolResource, lane: Lane) -> Self {
        let usage = UsageToken::exclusive(lane);
        Self {
            resource,
            lane,
            usage: Rc::new(RefCell::new(usage)),
        }
    }

    pub fn lane(&self) -> Lane {
        self.lane
    }

    pub(crate) fn swap_usage(&mut self) -> UsageToken {
        self.usage
            .replace_with(|_| UsageToken::exclusive(self.lane))
    }

    fn allocate(&mut self) -> Result<CommandBuffer> {
        todo!()
    }
}

pub struct CommandBuffer {
    lane: Lane,
    usage: Rc<RefCell<UsageToken>>,
}

impl CommandBuffer {
    fn into_packet(self) -> CommandPacket {
        todo!()
    }
}

const DEFAULT_SUBMISSION_SIZE: usize = 4;

pub struct Submission<const N: usize = DEFAULT_SUBMISSION_SIZE> {
    lane: Lane,
    usage: Rc<RefCell<UsageToken>>,
    packets: SmallVec<[CommandPacket; N]>,
}

impl<const N: usize> Submission<N> {
    fn new(factory: &impl SubmissionFactory) -> Self {
        Self {
            lane: factory.lane(),
            usage: factory.usage().clone(),
            packets: SmallVec::new(),
        }
    }

    fn with_capacity(factory: &impl SubmissionFactory, capacity: usize) -> Self {
        Self {
            lane: factory.lane(),
            usage: factory.usage().clone(),
            packets: SmallVec::with_capacity(capacity),
        }
    }

    fn push(&mut self, cmdbuf: CommandBuffer) {
        if self.packets.len() == self.packets.capacity() {
            println!("WARNING: submission allocation");
        }
        self.packets.push(cmdbuf.into_packet());
    }
}

trait SubmissionFactory {
    fn lane(&self) -> Lane;
    fn usage(&self) -> &Rc<RefCell<UsageToken>>;
}

impl SubmissionFactory for CommandPool {
    fn lane(&self) -> Lane {
        self.lane
    }

    fn usage(&self) -> &Rc<RefCell<UsageToken>> {
        &self.usage
    }
}

impl SubmissionFactory for CommandBuffer {
    fn lane(&self) -> Lane {
        self.lane
    }

    fn usage(&self) -> &Rc<RefCell<UsageToken>> {
        &self.usage
    }
}

struct CommandPacket {
    //
}

// fn test(mut device: Device) -> Result<()> {
//     // use crate::gal::*;

//     let request = GraphicsQueue::build();
//     let mut queue = device.create_queue(request)?;
//     let device = Arc::new(device);

//     let mut allocator = CommandAllocator::new(device.clone(), queue.lane(), 3);
//     let mut pool = allocator.acquire()?.unwrap();
//     let cmdbuf = pool.allocate()?;

//     let mut submission = Submission::new(&pool);
//     submission.push(cmdbuf);

//     queue.submit(submission)?;

//     Ok(())
// }
