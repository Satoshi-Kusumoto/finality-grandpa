// Copyright 2018 Parity Technologies (UK) Ltd.
// This file is part of finality-afg.

// finality-afg is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// finality-afg is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with finality-afg. If not, see <http://www.gnu.org/licenses/>.

//! Bridging round state between rounds.

use round::State as RoundState;
use futures::task;
use parking_lot::{RwLock, RwLockReadGuard};
use std::sync::Arc;

// round state bridged across rounds.
struct Bridged<H> {
	inner: RwLock<RoundState<H>>,
	task: task::AtomicTask,
}

impl<H> Bridged<H> {
	fn new(inner: RwLock<RoundState<H>>) -> Self {
		Bridged {
			inner,
			task: task::AtomicTask::new(),
		}
	}
}

/// A prior view of a round-state.
pub(crate) struct PriorView<H>(Arc<Bridged<H>>);

impl<H> PriorView<H> {
	/// Push an update to the latter view.
	pub(crate) fn update(&self, new: RoundState<H>) {
		*self.0.inner.write() = new;
		self.0.task.notify();
	}
}

/// A latter view of a round-state.
pub(crate) struct LatterView<H>(Arc<Bridged<H>>);

impl<H> LatterView<H> {
	/// Fetch a handle to the last round-state.
	pub(crate) fn get(&self) -> RwLockReadGuard<RoundState<H>> {
		self.0.task.register();
		self.0.inner.read()
	}
}

/// Constructs two views of a bridged round-state.
///
/// The prior view is held by a round which produces the state and pushes updates to a latter view.
/// When updating, the latter view's task is updated.
///
/// The latter view is held by the subsequent round, which blocks certain activity
/// while waiting for events on an older round.
pub(crate) fn bridge_state<H>(initial: RoundState<H>) -> (PriorView<H>, LatterView<H>) {
	let inner = Arc::new(Bridged::new(RwLock::new(initial)));
	(
		PriorView(inner.clone()), LatterView(inner)
	)
}

#[cfg(test)]
mod tests {
	use futures::prelude::*;
	use std::sync::Barrier;
	use super::*;

	#[test]
	fn bridging_state() {

		let initial = RoundState {
			prevote_ghost: None,
			finalized: None,
			estimate: None,
			completable: false,
		};

		let (prior, latter) = bridge_state(initial);
		let waits_for_finality = ::futures::future::poll_fn(move || -> Poll<(), ()> {
			if latter.get().finalized.is_some() {
				Ok(Async::Ready(()))
			} else {
				Ok(Async::NotReady)
			}
		});

		let barrier = Arc::new(Barrier::new(2));
		let barrier_other = barrier.clone();
		::std::thread::spawn(move || {
			barrier_other.wait();
			prior.update(RoundState {
				prevote_ghost: Some(("5", 5)),
				finalized: Some(("1", 1)),
				estimate: Some(("3", 3)),
				completable: true,
			});
		});

		barrier.wait();
		waits_for_finality.wait().unwrap();
	}
}
