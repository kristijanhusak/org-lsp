//! Commands that wait for Emacs without holding up the server. A command
//! the editor asked for (`workspace/executeCommand`) that needs an answer
//! of Emacs's (the menu of export, what `%(…)` of a capture template makes,
//! the block `org-edit-special` edits) sends Emacs the job and goes no
//! further: the request is kept, and the server answers the others
//! meanwhile. Once Emacs has answered, the request runs again from the
//! start, and the same call then has Emacs's answer at once; a command
//! that needs another answer waits again. A command asks Emacs nothing
//! before it has taken no step of its own, so running it again does what
//! running it once would have.
//!
//! The answers kept for a run are by the method and its parameters: a run
//! that asks something else (the file changed meanwhile) waits for that.
//! After [`RUNS`] runs, the command waits for Emacs as it did before,
//! holding up the server.
//!
//! `$/cancelRequest` gives up a request that waits: Emacs is stopped if
//! it is busy with the job (a code block that loops), and the request is
//! answered `RequestCancelled`.

use std::collections::HashMap;
use std::time::Duration;

use lsp_server::{Request, RequestId};
use serde_json::Value;

use crate::emacs::Emacs;

/// What a call that waits for Emacs fails with, for the command to stop
/// at: no one is told.
pub(super) const WAITING: &str = "Waiting for Emacs";

/// The most times a request is run again.
const RUNS: u32 = 8;

/// The tags of the jobs of requests that wait, apart from those of
/// `emacs_jobs`.
const FIRST_TAG: u64 = 1 << 62;

type Key = (String, String);

/// A request waiting for the job of its tag.
struct Held {
    request: Request,
    /// The call it waits for.
    key: Key,
    /// What Emacs answered the calls before, for the next run.
    answers: Vec<(Key, Result<Value, String>)>,
    runs: u32,
}

#[derive(Default)]
pub(super) struct Waits {
    /// The request being run, if it may wait.
    request: Option<Request>,
    /// Emacs's answers for the run, and those it used.
    answers: HashMap<Key, Result<Value, String>>,
    used: Vec<(Key, Result<Value, String>)>,
    runs: u32,
    /// The job the run sent Emacs, and its call.
    started: Option<(u64, Key)>,
    next: u64,
    held: HashMap<u64, Held>,
}

impl Waits {
    /// A request starts: it may wait for Emacs when it is one of the
    /// editor's (`request`).
    pub(super) fn begin(&mut self, request: Option<Request>) {
        self.request = request;
        self.started = None;
        self.used.clear();
    }

    /// Whether the run has sent Emacs a job it waits for.
    pub(super) fn waiting(&self) -> bool {
        self.started.is_some()
    }

    /// The request is done: true when it waits for Emacs, and is to be
    /// answered once Emacs has.
    pub(super) fn end(&mut self) -> bool {
        self.answers.clear();
        let runs = std::mem::take(&mut self.runs);
        let used = std::mem::take(&mut self.used);
        match (self.started.take(), self.request.take()) {
            (Some((tag, key)), Some(request)) => {
                self.held.insert(tag, Held { request, key, answers: used, runs: runs + 1 });
                true
            }
            _ => false,
        }
    }

    /// `method` in Emacs: its answer, if Emacs gave it for this run; else
    /// the job sent and [`WAITING`], for a request that may wait; else the
    /// answer waited for, at most `timeout` once Emacs has the job.
    pub(super) fn call(
        &mut self,
        emacs: &Emacs,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let key = (method.to_string(), params.to_string());
        if let Some(answer) = self.answers.remove(&key) {
            self.used.push((key, answer.clone()));
            return answer;
        }
        // Waiting already: the command goes no further.
        if self.started.is_some() {
            return Err(WAITING.to_string());
        }
        if self.request.is_some() && self.runs < RUNS {
            self.next += 1;
            let tag = FIRST_TAG + self.next;
            emacs.wait(tag, method, params, timeout);
            self.started = Some((tag, key));
            return Err(WAITING.to_string());
        }
        emacs.call(method, params, timeout)
    }

    /// Whether `tag` is the job of a request that waits.
    pub(super) fn owns(tag: u64) -> bool {
        tag > FIRST_TAG
    }

    /// Emacs answered the job of `tag`: the request that waited for it, to
    /// run again with the answers.
    pub(super) fn answered(&mut self, tag: u64, result: Result<Value, String>) -> Option<Request> {
        let held = self.held.remove(&tag)?;
        self.answers = held.answers.into_iter().collect();
        self.answers.insert(held.key, result);
        self.runs = held.runs;
        Some(held.request)
    }

    /// The request `id` given up: the tag of its job, which Emacs is to
    /// give up too.
    pub(super) fn cancel(&mut self, id: &RequestId) -> Option<u64> {
        let tag = self.held.iter().find(|(_, held)| held.request.id == *id).map(|(tag, _)| *tag)?;
        self.held.remove(&tag);
        Some(tag)
    }
}
