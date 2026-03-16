#[cfg(debug_assertions)]
use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak, atomic::AtomicU64};

use anyhow::{Context, Result};
use vulkanalia::vk::{self, DeviceV1_2, HasBuilder};

use crate::gpu::{DeviceId, LaneKey, QueueGroupTable, QueueGroupVec, VulkanHandle};

type FrameNumber = u64;
type TimelineValue = u64;

pub struct SubmissionProgress {
    device_id: DeviceId,
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    queue_groups: QueueGroupTable,
    // max_frame_in_flight: FrameNumber,

    // current frame
    current_frame: CurrentFrame,

    // frames that may still be in-progress on the host
    pending_frames: VecDeque<ActiveFrame>,

    // per-lane progress for frames that host-complete
    lane_progress: QueueGroupVec<LaneProgress>,
}

impl SubmissionProgress {
    pub fn new(
        device_id: DeviceId,
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        queue_groups: QueueGroupTable,
    ) -> Result<Self> {
        let token = FrameToken::new(device_id, 0, &queue_groups);
        let frame = ActiveFrame::new(0, &token);
        let lane_progress = QueueGroupVec::try_new(&queue_groups, |key| {
            let binding = queue_groups.get_binding(key).context("invalid lane key")?;
            let semaphore = binding.semaphore.clone();
            Ok(LaneProgress {
                semaphore,
                last_frame: 0,
                last_count: 0,
                host_complete: VecDeque::new(),
                device_complete: None,
            })
        })?;
        Ok(Self {
            device_id,
            device,
            queue_groups,
            // max_frame_in_flight: 0,
            current_frame: CurrentFrame { token, frame },
            pending_frames: VecDeque::new(),
            lane_progress,
        })
    }

    fn min_settled_frame(&self) -> Option<FrameNumber> {
        self.lane_progress
            .iter()
            .map(|(_, progress)| {
                progress
                    .device_complete
                    .as_ref()
                    .map(|entry| entry.frame)
                    .unwrap_or(u64::MAX)
            })
            .min()
            .and_then(|x| if x == u64::MAX { None } else { Some(x) })
    }

    pub fn frames_in_flight(&self) -> FrameNumber {
        match self.min_settled_frame() {
            Some(frame) => self.current_frame.frame.number - frame - 1,
            None => self.current_frame.frame.number,
        }
    }

    pub fn next(&mut self, settled: &SettledLanes) -> Result<FrameToken> {
        let current = &self.current_frame;
        let next_number = current.frame.number + 1;
        let next_token = FrameToken::new(self.device_id, next_number, &self.queue_groups);
        let next_frame = ActiveFrame::new(next_number, &next_token);
        let next = CurrentFrame {
            token: next_token.clone(),
            frame: next_frame,
        };
        let prev = std::mem::replace(&mut self.current_frame, next);
        self.pending_frames.push_back(prev.frame);
        self.update(settled)?;
        // self.max_frame_in_flight = self.max_frame_in_flight.max(next_number);
        Ok(next_token)
    }

    pub fn update(&mut self, settled: &SettledLanes) -> Result<()> {
        use vulkanalia::prelude::v1_2::*;

        while let Some(pending) = self.pending_frames.front() {
            if pending.alive.upgrade().is_some() {
                break;
            }

            let pending = self.pending_frames.pop_front().unwrap();

            for (key, progress) in self.lane_progress.iter_mut() {
                // since there are no references to any FrameTokens, it is safe
                // to read submissions
                let (_, count) = pending.submissions.get(key);

                // update the number of host-completed submissions to this lane
                progress.last_frame = pending.number;
                progress.last_count += count.load(Ordering::Relaxed);

                // this frame is device-complete when the semaphore signals value
                let entry = LaneEntry {
                    frame: pending.number,
                    value: progress.last_count,
                };
                progress.host_complete.push_back(entry);

                // broadcast host completion
                let (_, settled) = settled.lanes.get(key);
                settled
                    .host_complete
                    .store(pending.number, Ordering::Relaxed);
            }
        }

        for (key, progress) in self.lane_progress.iter_mut() {
            let mut current_value = 0;

            if !progress.host_complete.is_empty() {
                current_value = unsafe {
                    let semaphore = *progress.semaphore.raw();
                    self.device.raw().get_semaphore_counter_value(semaphore)?
                };
            }

            // check if host-complete frames are device-complete yet
            while let Some(entry) = progress.host_complete.front() {
                if current_value < entry.value {
                    break;
                }

                let entry = progress.host_complete.pop_front().unwrap();

                // broadcast device completion
                let (_, settled) = settled.lanes.get(key);
                settled
                    .device_complete
                    .store(entry.frame, Ordering::Relaxed);

                progress.device_complete = Some(entry);
            }
        }

        Ok(())
    }

    pub fn wait(&mut self, settled: &SettledLanes, until: FrameNumber) -> Result<bool> {
        loop {
            // update progress before the lane_progress loop
            self.update(settled)?;

            // if the min settled frame reached is >= the target frame, then can
            // return early
            match self.min_settled_frame() {
                Some(frame) if frame >= until => return Ok(true),
                _ => {}
            }

            // loop until either: all lanes are >= until *or* was able to wait
            // on at least one lane
            let mut all_lanes_ready = true;
            let mut did_wait = false;

            for (_, progress) in self.lane_progress.iter_mut() {
                if let Some(entry) = &progress.device_complete {
                    if entry.frame >= until {
                        // if the lane's last device-complete frame is already
                        // >= the target, then lane is ready
                        continue;
                    }
                }

                // since lane is not device-complete, it's not ready, but it
                // _may_ become ready after the next update()
                all_lanes_ready = false;

                // find the first host-complete frame that is >= the target, and
                // then wait on its semaphore
                let entry = progress
                    .host_complete
                    .iter()
                    .find(|entry| entry.frame >= until);

                let Some(entry) = entry else {
                    // no host-complete frames that can be waited on; caller
                    // needs to wait for the host to submit something on this
                    // lane
                    continue;
                };

                let semaphore = unsafe { *progress.semaphore.raw() };
                let semaphores = [semaphore];
                let values = [entry.value];

                let info = vk::SemaphoreWaitInfo::builder()
                    .semaphores(&semaphores)
                    .values(&values)
                    .build();

                // this lane will be ready after the next update()
                unsafe {
                    self.device.raw().wait_semaphores(&info, u64::MAX)?;
                };

                did_wait = true;
            }

            if all_lanes_ready {
                return Ok(true);
            }

            if !did_wait {
                // no lanes ready and did not wait on anything, this means some
                // frames are still being executed on the host and have not been
                // submitted yet; the caller needs to busy-wait
                return Ok(false);
            }
        }
    }

    pub fn current(&self) -> &CurrentFrame {
        &self.current_frame
    }
}

pub struct CurrentFrame {
    pub token: FrameToken,
    pub frame: ActiveFrame,
}

struct SettledLane {
    host_complete: AtomicU64,
    device_complete: AtomicU64,
}

pub struct SettledLanes {
    lanes: QueueGroupVec<SettledLane>,
}

impl SettledLanes {
    pub fn new(queue_groups: &QueueGroupTable) -> Self {
        Self {
            lanes: QueueGroupVec::new(queue_groups, || SettledLane {
                host_complete: AtomicU64::new(u64::MAX),
                device_complete: AtomicU64::new(u64::MAX),
            }),
        }
    }

    pub fn is_host_complete(&self, key: LaneKey, frame: FrameNumber) -> bool {
        let (_, lane) = self.lanes.get(key);
        let settled = lane.host_complete.load(Ordering::Relaxed);
        settled != u64::MAX && frame <= settled
    }

    pub fn is_device_complete(&self, key: LaneKey, frame: FrameNumber) -> bool {
        let (_, lane) = self.lanes.get(key);
        let settled = lane.device_complete.load(Ordering::Relaxed);
        settled != u64::MAX && frame <= settled
    }
}

pub struct ActiveFrame {
    // frame number; increments for each new frame
    pub number: FrameNumber,

    // weak reference to the current frame; used to track host completion
    alive: Weak<()>,

    // this frame's per-lane submission counts
    submissions: Arc<QueueGroupVec<AtomicU64>>,
}

impl ActiveFrame {
    fn new(number: FrameNumber, token: &FrameToken) -> Self {
        Self {
            number,
            alive: Arc::downgrade(&token.alive),
            submissions: token.submissions.clone(),
        }
    }
}

struct LaneProgress {
    semaphore: VulkanHandle<vk::Semaphore>,
    last_frame: FrameNumber,
    last_count: u64,

    // frames that completed on the host, for this lane
    host_complete: VecDeque<LaneEntry>,

    // the last frame that completed on the device, for this lane
    device_complete: Option<LaneEntry>,
}

struct LaneEntry {
    frame: FrameNumber,
    value: TimelineValue,
}

#[derive(Clone)]
pub struct FrameToken {
    device_id: DeviceId,
    number: u64,
    alive: Arc<()>,
    submissions: Arc<QueueGroupVec<AtomicU64>>,
    #[cfg(debug_assertions)]
    is_consumed: Cell<bool>,
}

impl FrameToken {
    fn new(device_id: DeviceId, number: u64, queue_groups: &QueueGroupTable) -> Self {
        Self {
            device_id,
            number,
            alive: Arc::new(()),
            submissions: Arc::new(QueueGroupVec::new(queue_groups, Default::default)),
            #[cfg(debug_assertions)]
            is_consumed: Cell::new(false),
        }
    }

    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }

    pub fn number(&self) -> u64 {
        self.number
    }

    pub(crate) fn consume(self, key: LaneKey) -> u64 {
        let (_, counter) = self.submissions.get(key);
        counter.fetch_add(1, Ordering::Relaxed);
        #[cfg(debug_assertions)]
        {
            self.is_consumed.set(true);
        }
        self.number
    }

    pub(crate) fn downgrade(self) -> FrameRef {
        FrameRef::new(self)
    }
}

impl Drop for FrameToken {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            debug_assert!(!self.is_consumed.get());
        }
    }
}

#[derive(Clone)]
pub(crate) struct FrameRef {
    device_id: DeviceId,
    number: u64,
    alive: Weak<()>,
}

impl FrameRef {
    fn new(token: FrameToken) -> Self {
        Self {
            device_id: token.device_id,
            number: token.number,
            alive: Arc::downgrade(&token.alive),
        }
    }

    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }

    pub fn number(&self) -> u64 {
        self.number
    }

    pub fn is_alive(&self) -> bool {
        self.alive.upgrade().is_some()
    }
}
