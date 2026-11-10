//! An environment's `provides` as the model declares it, `spindle.environment.Provision`s, built
//! from the capability and implementation pairs the admission cases are written in.

use serde_json::{Value, json};

/// Groups `pairs`, each `{capability, implementation}`, into one `{capability, implementations}`
/// per capability, each capability and each of its implementations in the order it first appears.
pub(crate) fn provisions(pairs: &Value) -> Value {
    let mut grouped: Vec<(Value, Vec<Value>)> = Vec::new();
    for pair in pairs.as_array().expect("a list of pairs") {
        let capability = pair["capability"].clone();
        let implementation = pair["implementation"].clone();
        match grouped.iter_mut().find(|(known, _)| *known == capability) {
            Some((_, implementations)) => implementations.push(implementation),
            None => grouped.push((capability, vec![implementation])),
        }
    }
    grouped
        .into_iter()
        .map(|(capability, implementations)| {
            json!({"capability": capability, "implementations": implementations})
        })
        .collect()
}
