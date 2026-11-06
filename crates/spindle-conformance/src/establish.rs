//! An `establish` step: a row the scenario arranges, written where its entity rests.

use std::collections::BTreeMap;

use ess_compiler::EssIr;
use ess_conformance::TargetError;
use ess_conformance::target::EntitySetupRequest;
use ess_primitives::node::Node;
use serde_json::Value;
use spindle_host::{Host, HostError};
use spindle_kernel::{Row, StoreError};

/// What every refusal of a setup is answered under, as ESS's interpreter names it.
const SETUP: &str = "entity setup";

/// Establishes the row `request` names on `host`, as ESS's interpreter does: checked against the
/// model by ESS ([`ess_conformance::input::validate_entity_setup`]), then written into the store
/// of the component that owns the entity ([`Host::establish`]) in the declared state, with its
/// fields and without history. A field present as `null` is held as absent, as the kernel holds
/// every field that holds nothing.
///
/// # Errors
/// `unavailable` for a row ESS refuses or an identity already held, as the interpreter answers
/// both; `unsupported` for an entity the host cannot place.
pub(crate) fn establish(
    ir: &EssIr,
    host: &mut Host,
    request: &EntitySetupRequest,
) -> Result<(), TargetError> {
    ess_conformance::input::validate_entity_setup(
        ir,
        &request.entity,
        &request.identity,
        &request.fields,
        &request.state,
    )
    .map_err(|detail| TargetError::unavailable(SETUP, detail))?;
    let entity = request.entity.name().to_string();
    let identity = json(&request.identity)?;
    let fields = request
        .fields
        .iter()
        .filter(|(_, value)| !matches!(value, Node::Null))
        .map(|(field, value)| json(value).map(|value| (field.clone(), value)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let row = Row {
        state: request.state.as_str().to_owned(),
        fields,
    };
    match host.establish(&entity, &identity, row) {
        Ok(()) => Ok(()),
        Err(HostError::Store(StoreError::Conflict { .. })) => Err(TargetError::unavailable(
            SETUP,
            "the entity identity is already established",
        )),
        Err(HostError::Unsupported(construct)) => Err(TargetError::unsupported(
            format!("establishing `{entity}`"),
            construct,
        )),
        Err(other) => Err(TargetError::unavailable(SETUP, other.to_string())),
    }
}

fn json(node: &Node) -> Result<Value, TargetError> {
    serde_json::to_value(node).map_err(|error| TargetError::unavailable(SETUP, error.to_string()))
}
