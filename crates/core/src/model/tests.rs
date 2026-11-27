//! Model-wide invariants: stable ids, labels and the display-only helpers.

use super::*;

#[test]
fn enum_ids_round_trip() {
    for kind in FeedKind::ALL {
        assert_eq!(FeedKind::parse(kind.as_str()).unwrap(), *kind);
    }
    for color in PoopColor::ALL {
        assert_eq!(PoopColor::parse(color.as_str()).unwrap(), *color);
    }
    for role in Role::ALL {
        assert_eq!(Role::parse(role.as_str()).unwrap(), *role);
    }
    for mood in Mood::ALL {
        assert_eq!(Mood::parse(mood.as_str()).unwrap(), *mood);
    }
}

#[test]
fn enum_ids_are_snake_case_and_match_labels() {
    assert_eq!(FeedKind::BreastLeft.as_str(), "breast_left");
    assert_eq!(FeedKind::BreastLeft.label(), "Breast left");
    assert_eq!(PoopColor::RedStreaked.as_str(), "red_streaked");
    assert_eq!(SleepLocation::ParentsBed.as_str(), "parents_bed");
    assert_eq!(Role::CoParent.as_str(), "co_parent");
    assert_eq!(Mood::HardDay.as_str(), "hard_day");
}

#[test]
fn unknown_enum_values_are_rejected() {
    assert!(FeedKind::parse("breast_up").is_err());
    assert!(PoopColor::parse("rainbow").is_err());
    assert!(Role::parse("admin").is_err());
}

#[test]
fn alert_colors_match_the_trends_warning() {
    assert!(PoopColor::RedStreaked.is_alert());
    assert!(PoopColor::Dark.is_alert());
    assert!(PoopColor::Pale.is_alert());
    assert!(!PoopColor::Mustard.is_alert());
}
