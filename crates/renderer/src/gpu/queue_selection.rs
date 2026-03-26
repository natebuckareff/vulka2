use std::collections::BTreeMap;

use anyhow::{Result, anyhow};

use crate::gpu::{QueueFamily, QueueFamilyId, QueueRoleFlags};

pub fn get_available_families(
    families: &BTreeMap<QueueFamilyId, QueueFamily>,
    reservations: &BTreeMap<QueueFamilyId, u32>,
) -> Result<Vec<QueueFamily>> {
    families
        .values()
        .copied()
        .map(|family| get_available_family(family, reservations))
        .collect()
}

fn get_available_family(
    mut family: QueueFamily,
    reservations: &BTreeMap<QueueFamilyId, u32>,
) -> Result<QueueFamily> {
    let reserved = reservations.get(&family.id).copied().unwrap_or(0);
    family.count = family.count.checked_sub(reserved).ok_or_else(|| {
        let id: u32 = family.id.into();
        anyhow!(
            "internal queue reservation state invalid for family {}: reserved={} count={}",
            id,
            reserved,
            family.count
        )
    })?;
    Ok(family)
}

pub fn select_best_families(families: &[QueueFamily], roles: QueueRoleFlags) -> Vec<QueueFamily> {
    let families = families
        .iter()
        .copied()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::gpu::{QueueFamily, QueueFamilyId, QueueRoleFlags};

    fn allocate_group(
        families: &mut Vec<QueueFamily>,
        roles: QueueRoleFlags,
    ) -> Vec<QueueFamilyId> {
        let selected = select_best_families(families, roles);

        for selected_family in &selected {
            let family = families
                .iter_mut()
                .find(|family| family.id == selected_family.id)
                .expect("selected family must exist");
            assert!(
                family.count > 0,
                "selected family must have available queues"
            );
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

        let best = select_best_families(
            &families,
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

        let best = select_best_families(
            &families,
            QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
        );

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

        let best = select_best_families(
            &families,
            QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
        );

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

        let best = select_best_families(
            &families,
            QueueRoleFlags::GRAPHICS | QueueRoleFlags::TRANSFER,
        );

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

        let best = select_best_families(
            &families,
            QueueRoleFlags::GRAPHICS | QueueRoleFlags::TRANSFER,
        );

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

        let best = select_best_families(
            &families,
            QueueRoleFlags::COMPUTE | QueueRoleFlags::TRANSFER,
        );

        assert_eq!(best.len(), 1);
        assert_eq!(best[0].id, QueueFamilyId::from(1u32));
    }

    #[test]
    fn project_available_families_errors_on_over_allocation() {
        let id0 = QueueFamilyId::from(0u32);

        let mut families = BTreeMap::new();
        families.insert(
            id0,
            QueueFamily {
                id: id0,
                count: 1,
                roles: QueueRoleFlags::TRANSFER,
            },
        );

        let mut allocations = BTreeMap::new();
        allocations.insert(id0, 2);

        let projected = get_available_families(&families, &allocations);
        assert!(projected.is_err());
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
