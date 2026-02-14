use anyhow::Result;
use bitflags::bitflags;
use vulkanalia::vk;

use crate::gpu_v2::DeviceBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueFamilyId(u32);

impl From<u32> for QueueFamilyId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<usize> for QueueFamilyId {
    fn from(value: usize) -> Self {
        Self(value as u32)
    }
}

impl Into<u32> for QueueFamilyId {
    fn into(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueId {
    pub family: QueueFamilyId,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFamily {
    pub id: QueueFamilyId,
    pub roles: QueueRoleFlags,
    pub count: u32,
}

pub enum QueueRole {
    Graphics,
    Compute,
    Transfer,
    Present,
}

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct QueueRoleFlags: u8 {
        const GRAPHICS = 0b0001;
        const COMPUTE  = 0b0010;
        const TRANSFER = 0b0100;
        const PRESENT  = 0b1000;
    }
}

impl From<vk::QueueFlags> for QueueRoleFlags {
    fn from(flags: vk::QueueFlags) -> Self {
        let mut roles = Self::empty();
        if flags.contains(vk::QueueFlags::GRAPHICS) {
            roles |= Self::GRAPHICS;
        }
        if flags.contains(vk::QueueFlags::COMPUTE) {
            roles |= Self::COMPUTE;
        }
        if flags.contains(vk::QueueFlags::TRANSFER) {
            roles |= Self::TRANSFER;
        }
        roles
    }
}

impl Into<QueueRoleFlags> for QueueRole {
    fn into(self) -> QueueRoleFlags {
        match self {
            QueueRole::Graphics => QueueRoleFlags::GRAPHICS,
            QueueRole::Compute => QueueRoleFlags::COMPUTE,
            QueueRole::Transfer => QueueRoleFlags::TRANSFER,
            QueueRole::Present => QueueRoleFlags::PRESENT,
        }
    }
}

pub struct QueueGroupBuilder<'a> {
    builder: &'a mut DeviceBuilder,
    roles: QueueRoleFlags,
}

impl<'a> QueueGroupBuilder<'a> {
    pub(crate) fn new(builder: &'a mut DeviceBuilder) -> Self {
        Self {
            builder,
            roles: QueueRoleFlags::empty(),
        }
    }

    pub fn graphics(mut self) -> Self {
        self.roles |= QueueRoleFlags::GRAPHICS;
        self
    }

    pub fn present(mut self) -> Self {
        self.roles |= QueueRoleFlags::PRESENT;
        self
    }

    pub fn compute(mut self) -> Self {
        self.roles |= QueueRoleFlags::COMPUTE;
        self
    }

    pub fn transfer(mut self) -> Self {
        self.roles |= QueueRoleFlags::TRANSFER;
        self
    }

    pub fn build(self) -> Result<Option<QueueGroup>> {
        if self.roles.is_empty() {
            return Ok(None);
        }

        let families = self.builder.available_queue_families();
        let selected = get_best_families(families, self.roles);
        if selected.is_empty() {
            return Ok(None);
        }

        let family_ids = selected
            .iter()
            .map(|family| family.id)
            .collect::<Vec<QueueFamilyId>>();

        let queue_ids = self.builder.reserve_queues(&family_ids)?;
        let grouped = family_ids
            .into_iter()
            .zip(queue_ids)
            .map(|(family_id, queue_id)| (family_id, queue_id, 1))
            .collect::<Vec<_>>();

        let queue_group = QueueGroup::new(grouped);
        Ok(Some(queue_group))
    }
}

#[derive(Debug, Clone)]
pub struct QueueGroup {
    families: Vec<(QueueFamilyId, QueueId, u32)>,
}

impl QueueGroup {
    fn new(families: Vec<(QueueFamilyId, QueueId, u32)>) -> Self {
        Self { families }
    }
}

fn get_fully_covering_families(
    families: &[QueueFamily],
    roles: QueueRoleFlags,
) -> Vec<QueueFamily> {
    families
        .iter()
        .copied()
        .filter(|family| family.roles.contains(roles))
        .collect()
}

fn get_coverage_ranked_families(
    families: &[QueueFamily],
    roles: QueueRoleFlags,
) -> Vec<QueueFamily> {
    let mut sorted = families.to_vec();
    sorted.sort_by(|a, b| {
        let coverage_a = a.roles.intersection(roles).bits().count_ones();
        let coverage_b = b.roles.intersection(roles).bits().count_ones();
        coverage_b.cmp(&coverage_a).then_with(|| a.id.cmp(&b.id))
    });

    sorted
        .into_iter()
        .filter(|family| !family.roles.intersection(roles).is_empty())
        .collect()
}

fn get_greedily_covering_families(
    families: &[QueueFamily],
    roles: QueueRoleFlags,
) -> Vec<QueueFamily> {
    let mut chosen = Vec::new();
    let mut remaining = roles;

    while !remaining.is_empty() {
        let ranked = get_coverage_ranked_families(families, remaining);
        let Some(best) = ranked.first().copied() else {
            return Vec::new();
        };

        chosen.push(best);
        remaining = remaining.difference(best.roles);
    }

    chosen
}

fn get_bias_ranked_families(families: &[QueueFamily]) -> Vec<QueueFamily> {
    let mut sorted = families.to_vec();
    sorted.sort_by(|a, b| {
        let role_count_a = a.roles.bits().count_ones();
        let role_count_b = b.roles.bits().count_ones();
        role_count_a
            .cmp(&role_count_b)
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.id.cmp(&b.id))
    });
    sorted
}

fn get_best_families(families: Vec<QueueFamily>, roles: QueueRoleFlags) -> Vec<QueueFamily> {
    let families = families
        .into_iter()
        .filter(|family| family.count > 0)
        .collect::<Vec<_>>();

    if families.is_empty() {
        return Vec::new();
    }

    let full_cover = get_fully_covering_families(&families, roles);
    if !full_cover.is_empty() {
        let ranked = get_bias_ranked_families(&full_cover);
        return ranked.into_iter().take(1).collect();
    }

    get_greedily_covering_families(&families, roles)
}

#[cfg(test)]
mod tests {
    use super::{QueueFamily, QueueFamilyId, QueueRoleFlags, get_best_families};

    fn allocate_group(
        families: &mut Vec<QueueFamily>,
        roles: QueueRoleFlags,
    ) -> Vec<QueueFamilyId> {
        let selected = get_best_families(families.clone(), roles);

        for selected_family in &selected {
            let family = families
                .iter_mut()
                .find(|family| family.id == selected_family.id)
                .expect("selected family must exist");
            assert!(family.count > 0, "selected family must have available queues");
            family.count -= 1;
        }

        selected.into_iter().map(|family| family.id).collect()
    }

    #[test]
    fn chooses_single_full_cover_family_when_available() {
        let families = vec![
            QueueFamily {
                id: QueueFamilyId::from(0u32),
                count: 16,
                roles: QueueRoleFlags::GRAPHICS
                    | QueueRoleFlags::COMPUTE
                    | QueueRoleFlags::TRANSFER
                    | QueueRoleFlags::PRESENT,
            },
            QueueFamily {
                id: QueueFamilyId::from(1u32),
                count: 2,
                roles: QueueRoleFlags::TRANSFER,
            },
            QueueFamily {
                id: QueueFamilyId::from(2u32),
                count: 8,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
            },
        ];

        let best = get_best_families(
            families,
            QueueRoleFlags::GRAPHICS | QueueRoleFlags::TRANSFER | QueueRoleFlags::PRESENT,
        );

        assert_eq!(best.len(), 1);
        assert_eq!(best[0].id, QueueFamilyId::from(0u32));
    }

    #[test]
    fn bias_prefers_fewer_roles_for_single_family_match() {
        let families = vec![
            QueueFamily {
                id: QueueFamilyId::from(0u32),
                count: 1,
                roles: QueueRoleFlags::GRAPHICS
                    | QueueRoleFlags::COMPUTE
                    | QueueRoleFlags::TRANSFER
                    | QueueRoleFlags::PRESENT,
            },
            QueueFamily {
                id: QueueFamilyId::from(1u32),
                count: 4,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER | QueueRoleFlags::PRESENT,
            },
        ];

        let best = get_best_families(families, QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER);

        assert_eq!(best.len(), 1);
        assert_eq!(best[0].id, QueueFamilyId::from(1u32));
    }

    #[test]
    fn full_cover_tie_break_prefers_lower_family_id() {
        let families = vec![
            QueueFamily {
                id: QueueFamilyId::from(2u32),
                count: 1,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
            },
            QueueFamily {
                id: QueueFamilyId::from(1u32),
                count: 1,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
            },
        ];

        let best = get_best_families(families, QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER);

        assert_eq!(best.len(), 1);
        assert_eq!(best[0].id, QueueFamilyId::from(1u32));
    }

    #[test]
    fn greedy_cover_selects_multiple_families_when_needed() {
        let families = vec![
            QueueFamily {
                id: QueueFamilyId::from(0u32),
                count: 1,
                roles: QueueRoleFlags::GRAPHICS,
            },
            QueueFamily {
                id: QueueFamilyId::from(1u32),
                count: 1,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
            },
        ];

        let best = get_best_families(families, QueueRoleFlags::GRAPHICS | QueueRoleFlags::TRANSFER);

        assert_eq!(best.len(), 2);
        assert_eq!(best[0].id, QueueFamilyId::from(0u32));
        assert_eq!(best[1].id, QueueFamilyId::from(1u32));
    }

    #[test]
    fn greedy_coverage_tie_break_prefers_lower_family_id() {
        let families = vec![
            QueueFamily {
                id: QueueFamilyId::from(2u32),
                count: 1,
                roles: QueueRoleFlags::GRAPHICS,
            },
            QueueFamily {
                id: QueueFamilyId::from(1u32),
                count: 1,
                roles: QueueRoleFlags::TRANSFER,
            },
        ];

        let best = get_best_families(families, QueueRoleFlags::GRAPHICS | QueueRoleFlags::TRANSFER);

        assert_eq!(best.len(), 2);
        assert_eq!(best[0].id, QueueFamilyId::from(1u32));
        assert_eq!(best[1].id, QueueFamilyId::from(2u32));
    }

    #[test]
    fn zero_count_families_are_ignored() {
        let families = vec![
            QueueFamily {
                id: QueueFamilyId::from(0u32),
                count: 0,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER | QueueRoleFlags::PRESENT,
            },
            QueueFamily {
                id: QueueFamilyId::from(1u32),
                count: 1,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
            },
        ];

        let best = get_best_families(families, QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER);

        assert_eq!(best.len(), 1);
        assert_eq!(best[0].id, QueueFamilyId::from(1u32));
    }

    #[test]
    fn sequential_groups_use_remaining_capacity_only() {
        let id0 = QueueFamilyId::from(0u32);
        let id1 = QueueFamilyId::from(1u32);
        let id2 = QueueFamilyId::from(2u32);

        let mut families = vec![
            QueueFamily {
                id: id0,
                count: 1,
                roles: QueueRoleFlags::GRAPHICS
                    | QueueRoleFlags::COMPUTE
                    | QueueRoleFlags::TRANSFER
                    | QueueRoleFlags::PRESENT,
            },
            QueueFamily {
                id: id1,
                count: 1,
                roles: QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
            },
            QueueFamily {
                id: id2,
                count: 1,
                roles: QueueRoleFlags::TRANSFER,
            },
        ];

        let primary = allocate_group(
            &mut families,
            QueueRoleFlags::GRAPHICS
                | QueueRoleFlags::COMPUTE
                | QueueRoleFlags::TRANSFER
                | QueueRoleFlags::PRESENT,
        );
        assert_eq!(primary, vec![id0]);

        let async_compute = allocate_group(
            &mut families,
            QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
        );
        assert_eq!(async_compute, vec![id1]);

        let async_transfer = allocate_group(&mut families, QueueRoleFlags::TRANSFER);
        assert_eq!(async_transfer, vec![id2]);
    }

    #[test]
    fn single_family_capacity_three_exhausts_on_fourth_group() {
        let id0 = QueueFamilyId::from(0u32);
        let mut families = vec![QueueFamily {
            id: id0,
            count: 3,
            roles: QueueRoleFlags::GRAPHICS
                | QueueRoleFlags::COMPUTE
                | QueueRoleFlags::TRANSFER
                | QueueRoleFlags::PRESENT,
        }];

        let first = allocate_group(
            &mut families,
            QueueRoleFlags::GRAPHICS
                | QueueRoleFlags::COMPUTE
                | QueueRoleFlags::TRANSFER
                | QueueRoleFlags::PRESENT,
        );
        assert_eq!(first, vec![id0]);

        let second = allocate_group(
            &mut families,
            QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
        );
        assert_eq!(second, vec![id0]);

        let third = allocate_group(&mut families, QueueRoleFlags::TRANSFER);
        assert_eq!(third, vec![id0]);

        let fourth = allocate_group(&mut families, QueueRoleFlags::TRANSFER);
        assert!(fourth.is_empty());
    }
}
