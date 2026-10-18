//! Role permissions — one source of truth for both the app and the server.
//!
//! | action                        | owner | co-parent | caregiver |
//! |-------------------------------|-------|-----------|-----------|
//! | view everything               | yes   | yes       | yes       |
//! | add entries                   | yes   | yes       | yes       |
//! | edit/delete own entries       | yes   | yes       | yes       |
//! | edit/delete anyone's entries  | yes   | yes       | no        |
//! | baby profile, invite people   | yes   | yes       | no        |
//! | delete all data, manage users | yes   | no        | no        |

use crate::model::Role;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Read entries, the baby profile and the caregiver list.
    View,
    /// Add a new entry (any caregiver).
    CreateEntry,
    /// Change an entry; `is_author` is true when the actor wrote it.
    EditEntry { is_author: bool },
    /// Soft-delete an entry; `is_author` is true when the actor wrote it.
    DeleteEntry { is_author: bool },
    /// Edit the baby's name/birthday/theme.
    ManageBaby,
    /// Create invite links.
    InviteCaregiver,
    /// Remove or change the role of other caregivers.
    ManageCaregivers,
    /// Wipe the whole log (tombstones for every entry).
    DeleteAllData,
}

impl Action {
    /// Actions a caregiver takes on their own rows.
    pub fn own_edit(self) -> Self {
        match self {
            Action::EditEntry { .. } => Action::EditEntry { is_author: true },
            Action::DeleteEntry { .. } => Action::DeleteEntry { is_author: true },
            other => other,
        }
    }
}

/// Can `role` do `action`?
pub fn can(role: Role, action: Action) -> bool {
    match role {
        Role::Owner => true,
        Role::CoParent => !matches!(action, Action::DeleteAllData | Action::ManageCaregivers),
        Role::Caregiver => matches!(
            action,
            Action::View
                | Action::CreateEntry
                | Action::EditEntry { is_author: true }
                | Action::DeleteEntry { is_author: true }
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_can_do_everything() {
        for action in [
            Action::View,
            Action::CreateEntry,
            Action::EditEntry { is_author: false },
            Action::DeleteEntry { is_author: false },
            Action::ManageBaby,
            Action::InviteCaregiver,
            Action::ManageCaregivers,
            Action::DeleteAllData,
        ] {
            assert!(
                can(Role::Owner, action),
                "owner should be allowed {action:?}"
            );
        }
    }

    #[test]
    fn co_parents_edit_everything_but_do_not_own_the_family() {
        assert!(can(Role::CoParent, Action::EditEntry { is_author: false }));
        assert!(can(
            Role::CoParent,
            Action::DeleteEntry { is_author: false }
        ));
        assert!(can(Role::CoParent, Action::ManageBaby));
        assert!(can(Role::CoParent, Action::InviteCaregiver));
        assert!(!can(Role::CoParent, Action::DeleteAllData));
        assert!(!can(Role::CoParent, Action::ManageCaregivers));
    }

    #[test]
    fn caregivers_view_and_add_but_only_touch_their_own_rows() {
        assert!(can(Role::Caregiver, Action::View));
        assert!(can(Role::Caregiver, Action::CreateEntry));
        assert!(can(Role::Caregiver, Action::EditEntry { is_author: true }));
        assert!(can(
            Role::Caregiver,
            Action::DeleteEntry { is_author: true }
        ));
        assert!(!can(
            Role::Caregiver,
            Action::EditEntry { is_author: false }
        ));
        assert!(!can(
            Role::Caregiver,
            Action::DeleteEntry { is_author: false }
        ));
        assert!(!can(Role::Caregiver, Action::ManageBaby));
        assert!(!can(Role::Caregiver, Action::InviteCaregiver));
        assert!(!can(Role::Caregiver, Action::DeleteAllData));
    }

    #[test]
    fn own_edit_keeps_other_actions_untouched() {
        assert_eq!(Action::View.own_edit(), Action::View);
        assert_eq!(
            Action::EditEntry { is_author: false }.own_edit(),
            Action::EditEntry { is_author: true }
        );
        assert!(can(
            Role::Caregiver,
            Action::EditEntry { is_author: false }.own_edit()
        ));
    }
}
