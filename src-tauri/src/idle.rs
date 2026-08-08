#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleState {
    Active,
    Idle { started_at_ms: i64 },
}

#[derive(Debug, Clone, Copy)]
pub struct IdlePolicy {
    threshold_seconds: u64,
}

impl IdlePolicy {
    pub fn new(threshold_seconds: u64) -> Self {
        Self {
            threshold_seconds: threshold_seconds.max(1),
        }
    }

    pub fn evaluate(
        &self,
        segment_started_at_ms: i64,
        sampled_at_ms: i64,
        idle_seconds: u64,
        media_playing: bool,
    ) -> IdleState {
        if media_playing || idle_seconds < self.threshold_seconds {
            return IdleState::Active;
        }

        let last_input_at_ms = sampled_at_ms.saturating_sub((idle_seconds as i64) * 1_000);
        IdleState::Idle {
            started_at_ms: last_input_at_ms.max(segment_started_at_ms),
        }
    }
}
