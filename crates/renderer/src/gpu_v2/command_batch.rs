use std::cell::OnceCell;

use anyhow::{Result, anyhow};

use crate::gpu_v2::{CommandBuffer, CommandBufferHandle, FrameToken, LaneVec};

pub struct CommandBatch {
    frame: Option<FrameToken>,
    lanes: OnceCell<LaneVec<Vec<CommandBufferHandle>>>,
}

impl CommandBatch {
    pub fn new() -> Self {
        Self {
            frame: None,
            lanes: OnceCell::new(),
        }
    }

    fn lanes(&mut self, cmdbuf: &CommandBuffer) -> &mut LaneVec<Vec<CommandBufferHandle>> {
        self.lanes.get_mut_or_init(|| {
            let lanes = cmdbuf.lanes();
            LaneVec::filled(lanes, || vec![])
        })
    }

    pub fn is_empty(&self) -> bool {
        self.frame.is_none()
    }

    pub fn add(&mut self, cmdbuf: CommandBuffer) {
        match &self.frame {
            Some(frame) => {
                debug_assert!(frame.device_id() == cmdbuf.frame().device_id());
                debug_assert!(frame.number() == cmdbuf.frame().number());
            }
            None => {
                // OPTIMIZE: slightly unnessesary clone; seems worth trade-off
                self.frame = Some(cmdbuf.frame().clone());
            }
        };
        let batch_lanes = self.lanes(&cmdbuf);
        let cmdbuf_lanes = cmdbuf.take_lanes();
        debug_assert!(batch_lanes.queue_group_id() == cmdbuf_lanes.queue_group_id());
        for (key, cmdbuf_lane) in cmdbuf_lanes.into_entries() {
            if !cmdbuf_lane.is_dirty() {
                continue;
            }
            let handle = cmdbuf_lane.take_handle();
            let batch_lane = batch_lanes.get_mut(key);
            batch_lane.push(handle);
        }
    }

    pub fn finish(mut self) -> Result<Submission> {
        // TODO: need to decide on error handling in general; there are a lot of
        // checks that should probably be asserts instead of results
        let Some(frame) = self.frame else {
            return Err(anyhow!("command batch has no frame"));
        };
        let Some(lanes) = self.lanes.take() else {
            return Err(anyhow!("command batch is empty"));
        };
        Ok(Submission { frame, lanes })
    }
}

pub struct Submission {
    pub(crate) frame: FrameToken,
    pub(crate) lanes: LaneVec<Vec<CommandBufferHandle>>,
}
