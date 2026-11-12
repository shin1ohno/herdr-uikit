//! The production bounded queue. No callbacks, handles, pointers or payloads.
use super::types::*;

pub(super) struct Queue {
    pub run: ProcessRun,
    pub epoch: u64,
    generation: u64,
    invocation: u64,
    pub sequence: u64,
    pub enabled: bool,
    pub registrations: [Option<Arm>; VIEW_CAPACITY],
    records: [Option<Record>; RECORD_CAPACITY],
    head: usize,
    pub len: usize,
    stack: [Option<Record>; INVOCATION_CAPACITY],
    pub depth: usize,
}
impl Queue {
    pub fn new(run: ProcessRun) -> Self {
        Self {
            run,
            epoch: 0,
            generation: 0,
            invocation: 0,
            sequence: 0,
            enabled: false,
            registrations: [None; VIEW_CAPACITY],
            records: [None; RECORD_CAPACITY],
            head: 0,
            len: 0,
            stack: [None; INVOCATION_CAPACITY],
            depth: 0,
        }
    }
    fn next(counter: &mut u64) -> Result<u64, ObserverError> {
        *counter = counter
            .checked_add(1)
            .ok_or(ObserverError::CounterOverflow)?;
        Ok(*counter)
    }
    pub fn arm(
        &mut self,
        run: ProcessRun,
        phase: Phase,
        generation: u64,
    ) -> Result<Arm, ObserverError> {
        if run != self.run {
            return Err(ObserverError::StaleIdentity);
        }
        if self.depth != 0 {
            return Err(ObserverError::Outstanding);
        }
        if self.len != 0 {
            return Err(ObserverError::Undrained);
        }
        let slot = if generation == 0 {
            self.registrations
                .iter()
                .position(Option::is_none)
                .ok_or(ObserverError::Capacity)?
        } else {
            self.registrations
                .iter()
                .position(|a| a.is_some_and(|a| a.view_generation == generation))
                .ok_or(ObserverError::StaleIdentity)?
        };
        let view_generation = if generation == 0 {
            Self::next(&mut self.generation)?
        } else {
            generation
        };
        let epoch = Self::next(&mut self.epoch)?;
        let arm = Arm {
            run,
            epoch,
            view_generation,
            phase,
        };
        self.registrations[slot] = Some(arm);
        self.enabled = true;
        Ok(arm)
    }
    pub fn contains(&self, arm: Arm) -> bool {
        arm.epoch == self.epoch && self.registrations.contains(&Some(arm))
    }
    pub fn enter(
        &mut self,
        generation: u64,
        action: Action,
        responder: ResponderOrigin,
    ) -> Result<Record, ObserverError> {
        if !self.enabled {
            return Err(ObserverError::Disabled);
        }
        let arm = self
            .registrations
            .iter()
            .flatten()
            .find(|a| a.view_generation == generation && a.epoch == self.epoch)
            .copied()
            .ok_or(ObserverError::StaleIdentity)?;
        // Every outstanding entry owns one slot that nested work cannot steal.
        if self.depth == INVOCATION_CAPACITY || self.len + self.depth + 2 > RECORD_CAPACITY {
            return Err(ObserverError::Capacity);
        }
        let invocation = Self::next(&mut self.invocation)?;
        let parent = self
            .depth
            .checked_sub(1)
            .and_then(|i| self.stack[i])
            .map(|r| r.key);
        let record = Record {
            schema_version: SCHEMA_VERSION,
            key: ObservationKey { arm, invocation },
            parent,
            action,
            responder,
            decision: None,
            event: Event::Entry,
        };
        self.push(record);
        self.stack[self.depth] = Some(record);
        self.depth += 1;
        Ok(record)
    }
    fn push(&mut self, record: Record) {
        self.records[(self.head + self.len) % RECORD_CAPACITY] = Some(record);
        self.len += 1;
    }
    pub fn complete(
        &mut self,
        entry: Record,
        decision: Option<Decision>,
        outcome: Outcome,
    ) -> Result<(), InvalidReason> {
        let Some(top) = self.depth.checked_sub(1) else {
            return Err(InvalidReason::Duplicate);
        };
        if self.stack[top] != Some(entry) {
            return Err(InvalidReason::Duplicate);
        }
        let outcome = if !self.enabled || !self.contains(entry.key.arm) {
            Outcome::Invalidated
        } else {
            outcome
        };
        let valid = match outcome {
            Outcome::Refused => decision.is_some_and(|d| !d.allowed()),
            Outcome::Dispatched {
                key_down_handled,
                fallback,
            } => {
                decision.is_some_and(Decision::allowed)
                    && (fallback == Fallback::Invoked)
                        == (entry.action == Action::Paste && !key_down_handled)
            }
            Outcome::Aborted | Outcome::Invalidated => true,
        };
        if !valid {
            return Err(InvalidReason::InvalidOutcome);
        }
        self.stack[top] = None;
        self.depth = top;
        // The reservation was included in every admission capacity calculation.
        self.push(Record {
            decision,
            event: Event::Outcome(outcome),
            ..entry
        });
        Ok(())
    }
    pub fn remove(&mut self, generation: u64) -> bool {
        if let Some(slot) = self
            .registrations
            .iter_mut()
            .find(|a| a.is_some_and(|a| a.view_generation == generation))
        {
            *slot = None;
            true
        } else {
            false
        }
    }
    pub fn copy_records(&self) -> [Option<Record>; RECORD_CAPACITY] {
        let mut result = [None; RECORD_CAPACITY];
        for (i, item) in result.iter_mut().take(self.len).enumerate() {
            *item = self.records[(self.head + i) % RECORD_CAPACITY];
        }
        result
    }
    pub fn clear_records(&mut self) {
        self.records = [None; RECORD_CAPACITY];
        self.head = 0;
        self.len = 0;
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
