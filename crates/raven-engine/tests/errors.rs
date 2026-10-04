use raven_engine::{BlockPtr, Network, PositionError, next_block_number};

#[test]
/// Verifies that position errors convert at the framework interface without losing the variant.
fn position_errors_convert_at_the_framework_interface_without_losing_the_variant() {
    /// Propagates a position error through the framework result boundary.
    fn resume() -> raven_engine::RavenResult<u64> {
        let network = Network::<u64, u64>::new(1, 0, None)?;
        Ok(next_block_number(
            0,
            Some(&network),
            Some(&BlockPtr {
                number: u64::MAX,
                hash: 1,
            }),
        )?)
    }
    assert!(matches!(
        resume(),
        Err(raven_engine::RavenError::Position(
            PositionError::HeightOverflow
        ))
    ));
}
