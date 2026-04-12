use std::cell::OnceCell;

use crate::gal::{
    device_builder::{DeviceRequest, QueueKind},
    device_info::{DeviceInfo, QueueFamilyInfo},
    queue::QueueFamily,
};

pub struct RankedDevice<'a> {
    info: &'a DeviceInfo,
    solution: Solution<'a>,
}

impl<'a> RankedDevice<'a> {
    pub(crate) fn into_parts(self) -> (&'a DeviceInfo, Solution<'a>) {
        (self.info, self.solution)
    }
}

pub(crate) fn get_ranked_devices<'a>(
    request: &'a DeviceRequest,
    infos: &'a Vec<DeviceInfo>,
) -> Vec<RankedDevice<'a>> {
    let mut ranking = vec![];
    for info in infos {
        if let Some(name) = &request.name {
            if !info.name.contains(name) {
                continue;
            }
        }
        if request.kind.is_some() && request.kind != info.kind {
            continue;
        }
        let state = State { info, request };
        let solution = Solution::new(state);
        let Some(solution) = solution.solve(0) else {
            continue;
        };
        ranking.push(RankedDevice { info, solution });
    }
    ranking.sort_by(|a, b| b.solution.score().total_cmp(&a.solution.score()));
    ranking
}

#[derive(Clone)]
pub struct Solution<'a> {
    state: State<'a>,
    allocations: Vec<Allocation>,
    score: OnceCell<f32>,
}

impl<'a> Solution<'a> {
    fn new(state: State<'a>) -> Self {
        Self {
            state,
            allocations: Default::default(),
            score: Default::default(),
        }
    }

    pub(crate) fn allocations(&self) -> &Vec<Allocation> {
        &self.allocations
    }

    fn solve(self, index: usize) -> Option<Solution<'a>> {
        if index >= self.state.request.queues.len() {
            // in the base case, return None if the full solution does actually
            // fullfill the full request
            return self.fulfills().then_some(self);
        }

        let request = self.state.request.queues[index];
        let mut candidates = vec![];
        for (family_index, info) in self.state.info.families.iter().enumerate() {
            let priority = self.candidate_priority(request, info);
            if priority == 0.0 {
                continue;
            }
            candidates.push((priority, family_index))
        }
        candidates.sort_by(|(a, _), (b, _)| b.total_cmp(a));

        let mut best: Option<(f32, Solution)> = None;

        for (_, family_index) in candidates {
            let info = &self.state.info.families[family_index];
            let family = info.family();
            let queue = self.next_queue(info);
            let present = self.state.request.surface.is_some() && info.supports_present();

            let current = Allocation {
                request,
                family,
                queue,
                present,
            };

            let mut child = self.clone();
            child.allocations.push(current);

            if let Some(solution) = child.solve(index + 1) {
                let child_score = solution.score();
                let best_score = *best.as_ref().map(|(score, _)| score).unwrap_or(&0.0);
                if child_score > best_score {
                    best = Some((child_score, solution))
                }
            }
        }

        best.map(|(_, best)| best)
    }

    fn fulfills(&self) -> bool {
        self.allocations.len() == self.state.request.queues.len()
            && (self.state.request.surface.is_none()
                || self.allocations.iter().any(|alloc| alloc.present))
    }

    fn next_queue(&self, info: &QueueFamilyInfo) -> u32 {
        info.count() - self.family_size(info)
    }

    fn family_size(&self, info: &QueueFamilyInfo) -> u32 {
        let mut count = info.count();
        for alloc in &self.allocations {
            if alloc.family == info.family() {
                count -= 1;
            }
        }
        count
    }

    fn score(&self) -> f32 {
        *self.score.get_or_init(|| {
            let mut score = 0.0;
            for alloc in &self.allocations {
                let info = self
                    .state
                    .info
                    .families
                    .iter()
                    .find(|info| info.family() == alloc.family)
                    .expect("invalid solver state");
                score += self.score_allocation(alloc.request, info, false);
            }
            score
        })
    }

    fn candidate_priority(&self, request: QueueKind, info: &QueueFamilyInfo) -> f32 {
        if self.family_size(info) == 0 {
            return 0.0;
        }
        self.score_allocation(request, info, true)
    }

    fn score_allocation(
        &self,
        request: QueueKind,
        info: &QueueFamilyInfo,
        include_candidate: bool,
    ) -> f32 {
        let graphics = info.supports_graphics();
        let compute = info.supports_compute();
        let transfer = info.supports_transfer();

        let mut sharing = if include_candidate { 1.0 } else { 0.0 };
        for alloc in &self.allocations {
            if alloc.family == info.family() {
                sharing += 1.0;
            }
        }
        let share_weight = if sharing == 0.0 {
            0.0
        } else {
            1.0 - 1.0 / sharing
        };

        let score = match request {
            QueueKind::Graphics | QueueKind::GraphicsCompute => {
                if !graphics || (request == QueueKind::GraphicsCompute && !compute) {
                    return 0.0;
                }
                if self.state.request.surface.is_some() {
                    // prefer surface support
                    if info.supports_present() { 2.0 } else { 1.0 }
                } else {
                    1.0
                }
            }
            QueueKind::Compute => {
                if !compute {
                    return 0.0;
                }
                // prefer non-graphics
                if !graphics { 2.0 } else { 1.0 }
            }
            QueueKind::Transfer => {
                if !transfer {
                    return 0.0;
                }
                if !graphics && !compute {
                    // prefer non-graphics and non-compute
                    3.0
                } else if !graphics {
                    // otherwise, prefer compute over graphics
                    2.0
                } else {
                    1.0
                }
            }
        };

        score + share_weight
    }
}

#[derive(Clone, Copy)]
struct State<'a> {
    info: &'a DeviceInfo,
    request: &'a DeviceRequest,
}

#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    pub request: QueueKind,
    pub family: QueueFamily,
    pub queue: u32,
    pub present: bool,
}
