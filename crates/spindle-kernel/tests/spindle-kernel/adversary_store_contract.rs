//! Adversary, story browser-store: the memory and SQLite stores driven by one seeded sequence of
//! creations, changes and removals, each naming a version the store itself answered earlier. Both
//! must answer every step alike (commit or conflict), and a conflict must name, absolutely, the
//! version the step named and the version the store held just before it (`None` when nothing is
//! held), as the contract's `StoreError::Conflict` fields document.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use spindle_kernel::{Change, Event, MemoryStore, Removal, Row, SqliteStore, Store, StoreError};

const ENTITY: &str = "adversary.tasks.Task";

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Committed,
    Conflict { named: Option<u64>, held: bool },
}

/// One store and every version it has answered per identity, in order.
struct Driven<'a> {
    store: &'a mut dyn Store,
    seen: BTreeMap<String, Vec<u64>>,
}

impl Driven<'_> {
    fn held(&mut self, identity: &Value) -> Option<u64> {
        let held = self
            .store
            .load(ENTITY, identity)
            .expect("loads")
            .map(|stored| stored.version);
        if let Some(version) = held {
            let seen = self.seen.entry(identity.to_string()).or_default();
            if seen.last() != Some(&version) {
                seen.push(version);
            }
        }
        held
    }

    /// The version at `index` of those this store answered for `identity`, or one past every
    /// version it answered when it answered none at that index.
    fn named(&self, identity: &Value, index: usize) -> u64 {
        let seen = self
            .seen
            .get(&identity.to_string())
            .cloned()
            .unwrap_or_default();
        seen.get(index)
            .copied()
            .unwrap_or_else(|| seen.last().copied().unwrap_or(0) + 1_000)
    }

    fn step(&mut self, step: u64, op: u64, identity: &Value, index: usize) -> Answer {
        let before = self.held(identity);
        let row = Row {
            state: "Open".to_owned(),
            fields: BTreeMap::from([("step".to_owned(), json!(step))]),
        };
        let events = vec![Event {
            name: "adversary.tasks.Touched".to_owned(),
            payload: Map::from_iter([("step".to_owned(), json!(step))]),
        }];
        let (named, result) = match op {
            0 => (
                None,
                self.store.commit(&Change {
                    entity: ENTITY.to_owned(),
                    identity: identity.clone(),
                    expected: None,
                    row,
                    events,
                    scanned: Vec::new(),
                }),
            ),
            1 => {
                let version = self.named(identity, index);
                (
                    Some(version),
                    self.store.commit(&Change {
                        entity: ENTITY.to_owned(),
                        identity: identity.clone(),
                        expected: Some(version),
                        row,
                        events,
                        scanned: Vec::new(),
                    }),
                )
            }
            _ => {
                let version = self.named(identity, index);
                (
                    Some(version),
                    self.store.remove(&Removal {
                        entity: ENTITY.to_owned(),
                        identity: identity.clone(),
                        expected: version,
                        events,
                        scanned: Vec::new(),
                    }),
                )
            }
        };
        let answer = match result {
            Ok(()) => Answer::Committed,
            Err(StoreError::Conflict {
                expected, actual, ..
            }) => {
                assert_eq!(
                    expected, named,
                    "step {step}: the conflict names the version the step named"
                );
                assert_eq!(
                    actual, before,
                    "step {step}: the conflict names the version held just before"
                );
                Answer::Conflict {
                    named,
                    held: actual.is_some(),
                }
            }
            Err(other) => panic!("step {step}: {other}"),
        };
        self.held(identity);
        answer
    }
}

#[test]
fn memory_and_sqlite_answer_a_seeded_sequence_alike_and_name_absolute_versions() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut sqlite =
        SqliteStore::open(&directory.path().join("adversary.sqlite")).expect("SQLite opens");
    let mut memory = MemoryStore::default();
    let mut memory = Driven {
        store: &mut memory,
        seen: BTreeMap::new(),
    };
    let mut sqlite = Driven {
        store: &mut sqlite,
        seen: BTreeMap::new(),
    };
    let identities = [json!("a"), json!("b"), json!(1)];
    let mut random = Lcg(0x5eed_b10c);
    for step in 0..400 {
        let identity = &identities[random.below(3) as usize];
        let op = random.below(3);
        let index = random.below(6) as usize;
        let on_memory = memory.step(step, op, identity, index);
        let on_sqlite = sqlite.step(step, op, identity, index);
        let same = match (&on_memory, &on_sqlite) {
            (Answer::Committed, Answer::Committed) => true,
            (Answer::Conflict { held: a, .. }, Answer::Conflict { held: b, .. }) => a == b,
            _ => false,
        };
        assert!(
            same,
            "step {step} ({op} on {identity} at index {index}): memory {on_memory:?}, SQLite {on_sqlite:?}"
        );
        let scanned = |driven: &mut Driven<'_>| {
            driven
                .store
                .scan(ENTITY)
                .expect("scans")
                .into_iter()
                .map(|(identity, stored)| (identity, stored.row))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            scanned(&mut memory),
            scanned(&mut sqlite),
            "step {step}: both stores hold the same rows"
        );
    }
}

/// The memory store with one defect: every conflict on a held instance names one past the version
/// held. `StoreError::Conflict::actual` is documented as "the version
/// the store holds"; a contract the three stores are measured by should refuse this store.
#[derive(Default)]
struct MisnamesTheHeldVersion(MemoryStore);

fn misnamed(result: Result<(), StoreError>) -> Result<(), StoreError> {
    result.map_err(|error| match error {
        StoreError::Conflict {
            entity,
            identity,
            expected,
            actual,
        } => StoreError::Conflict {
            entity,
            identity,
            expected,
            actual: actual.map(|version| version + 1),
        },
        other => other,
    })
}

impl Store for MisnamesTheHeldVersion {
    fn load(
        &mut self,
        entity: &str,
        identity: &Value,
    ) -> Result<Option<spindle_kernel::Stored>, StoreError> {
        self.0.load(entity, identity)
    }
    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        misnamed(self.0.commit(change))
    }
    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        misnamed(self.0.remove(removal))
    }
    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.0.scan(entity)
    }
}

#[test]
fn the_contract_refuses_a_store_whose_conflicts_misname_the_held_version() {
    let refused: Vec<&str> = spindle_kernel::contract::CASES
        .iter()
        .filter(|case| {
            std::panic::catch_unwind(|| (case.run)(&mut MisnamesTheHeldVersion::default())).is_err()
        })
        .map(|case| case.name)
        .collect();
    assert!(
        !refused.is_empty(),
        "every one of the {} contract cases passes a store whose every conflict names a version it does not hold",
        spindle_kernel::contract::CASES.len()
    );
}
