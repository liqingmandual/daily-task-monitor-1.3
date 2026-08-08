use daily_task_monitor_core::idle::{IdlePolicy, IdleState};

#[test]
fn six_minutes_without_input_backfills_to_last_input() {
    let policy = IdlePolicy::new(360);
    let state = policy.evaluate(1_000_000, 1_500_000, 420, false);

    assert_eq!(
        state,
        IdleState::Idle {
            started_at_ms: 1_080_000
        }
    );
}

#[test]
fn idle_backfill_never_starts_before_the_current_segment() {
    let policy = IdlePolicy::new(360);
    let state = policy.evaluate(1_200_000, 1_500_000, 420, false);

    assert_eq!(
        state,
        IdleState::Idle {
            started_at_ms: 1_200_000
        }
    );
}

#[test]
fn media_playback_exempts_passive_viewing_from_idle() {
    let policy = IdlePolicy::new(360);
    let state = policy.evaluate(1_000_000, 1_500_000, 420, true);

    assert_eq!(state, IdleState::Active);
}

#[test]
fn time_below_threshold_remains_active() {
    let policy = IdlePolicy::new(360);
    let state = policy.evaluate(1_000_000, 1_300_000, 300, false);

    assert_eq!(state, IdleState::Active);
}
