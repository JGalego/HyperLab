//! Which stacks, which tools, and what the user was asked.
//!
//! A tool call arrives from somewhere the user cannot see — another process,
//! a model, a script. This module is the one place that decides whether it
//! runs, and it answers three separate questions, because they fail
//! differently:
//!
//! * **Which stacks.** A policy can be written for one stack, so handing a
//!   client an address does not hand it every document that is ever opened.
//! * **Which tools.** A reader that is only supposed to read is held to it by
//!   the tool's own [`Access`], not by hoping it behaves.
//! * **What the user was asked.** Consent is a decision a person makes, so it
//!   is recorded as one: every [`Decision`] says whether anyone was asked and
//!   what they said.
//!
//! Nothing here reaches a network or a stack. It is a decision and a record,
//! which is why it can be tested by asserting on values.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Whether a tool only looks at a stack, or changes it.
///
/// Every tool declares this, so "read only" is a property of the tool table
/// rather than a list of names kept in step by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    /// Looks, and changes nothing.
    Read,
    /// Changes the stack. Undoably — but a change all the same.
    Write,
}

impl Access {
    /// Whether this changes the stack.
    #[must_use]
    pub const fn writes(self) -> bool {
        matches!(self, Self::Write)
    }
}

/// When a person has to be asked before a tool runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Consent {
    /// Never ask. For a caller the user is already driving by hand.
    #[default]
    NotNeeded,
    /// Ask before anything that changes the stack.
    BeforeWriting,
    /// Ask before every call, including reads.
    BeforeEverything,
}

/// What a person is being asked to allow.
///
/// Everything an [`Approver`] needs to write a sentence a human can answer,
/// and nothing it could use to do the thing itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approval<'a> {
    /// The tool that wants to run.
    pub tool: &'a str,
    /// What it would do.
    pub access: Access,
    /// The stack it would do it to.
    pub stack: &'a str,
}

/// Asked when a policy needs a person.
pub trait Approver {
    /// Returns whether this may go ahead.
    ///
    /// A caller with nobody to ask should return `false`: an unattended
    /// process must not be able to consent on a user's behalf.
    fn approve(&mut self, request: &Approval<'_>) -> bool;
}

/// Refuses everything that needs consent.
///
/// The right approver for anything running unattended, and the default.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyAll;

impl Approver for DenyAll {
    fn approve(&mut self, _request: &Approval<'_>) -> bool {
        false
    }
}

/// Consents to everything.
///
/// Only honest when a person really is driving, or in a test.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAll;

impl Approver for AllowAll {
    fn approve(&mut self, _request: &Approval<'_>) -> bool {
        true
    }
}

/// Why a call was refused, or that it was not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "camelCase")]
pub enum Verdict {
    /// It may run.
    Allowed,
    /// It may not, for this reason — which is written to be shown to a
    /// person, because it usually is.
    Refused {
        /// What to tell whoever asked.
        reason: String,
    },
}

impl Verdict {
    /// Whether the call may go ahead.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// One decision, kept so that it can be shown afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    /// The tool that was asked for.
    pub tool: String,
    /// What it would have done.
    pub access: Access,
    /// The stack it was aimed at.
    pub stack: String,
    /// Whether a person was actually asked. A refusal nobody was consulted
    /// about reads very differently from one somebody made.
    pub asked: bool,
    /// What was decided.
    pub verdict: Verdict,
}

/// What a caller may do, and when a person must be asked first.
///
/// Deliberately closed by default in the ways that matter: [`Policy::new`]
/// grants reading, and everything beyond that is opted into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Tools that may run at all. `None` means every tool HyperLab offers.
    tools: Option<BTreeSet<String>>,
    /// Whether tools that change the stack may run.
    writes: bool,
    /// Stacks this policy covers, by name. `None` means any stack.
    stacks: Option<BTreeSet<String>>,
    /// When to ask a person.
    consent: Consent,
    /// Tools a person has already said yes to, so they are asked once per
    /// tool rather than once per call.
    granted: BTreeSet<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy {
    /// Reading, and nothing else.
    ///
    /// The safe default: a caller that has said nothing about what it needs
    /// can look at a stack and cannot touch it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: None,
            writes: false,
            stacks: None,
            consent: Consent::NotNeeded,
            granted: BTreeSet::new(),
        }
    }

    /// Everything, with nobody asked.
    ///
    /// For a caller the user is driving directly, and for tests.
    #[must_use]
    pub fn trusted() -> Self {
        Self {
            writes: true,
            ..Self::new()
        }
    }

    /// Everything, once a person has said yes to each tool that writes.
    #[must_use]
    pub fn supervised() -> Self {
        Self {
            writes: true,
            consent: Consent::BeforeWriting,
            ..Self::new()
        }
    }

    /// Restricts this policy to the named tools.
    ///
    /// A name that is not a tool is kept rather than rejected: a policy is
    /// written once and the tool table grows, and silently widening a policy
    /// because a name has not been implemented yet would be the wrong way
    /// round.
    #[must_use]
    pub fn only<I, S>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tools = Some(tools.into_iter().map(Into::into).collect());
        self
    }

    /// Restricts this policy to stacks with these names.
    #[must_use]
    pub fn for_stacks<I, S>(mut self, stacks: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.stacks = Some(stacks.into_iter().map(Into::into).collect());
        self
    }

    /// Allows tools that change the stack.
    #[must_use]
    pub const fn allowing_writes(mut self) -> Self {
        self.writes = true;
        self
    }

    /// Sets when a person must be asked.
    #[must_use]
    pub const fn asking(mut self, consent: Consent) -> Self {
        self.consent = consent;
        self
    }

    /// Whether this tool is refused outright, whatever anyone says.
    ///
    /// Only the decisions that cannot be changed by consent: the allowlist
    /// and the ban on writing. Useful for deciding what to *offer* a caller,
    /// since a tool it could never call is worse than no tool at all.
    #[must_use]
    pub fn would_always_refuse(&self, tool: &str, access: Access) -> bool {
        let outside = self
            .tools
            .as_ref()
            .is_some_and(|tools| !tools.contains(tool));
        outside || (access.writes() && !self.writes)
    }

    /// Whether a person would be asked about a tool with this access.
    #[must_use]
    pub fn needs_consent(&self, tool: &str, access: Access) -> bool {
        let wanted = match self.consent {
            Consent::NotNeeded => false,
            Consent::BeforeWriting => access.writes(),
            Consent::BeforeEverything => true,
        };
        wanted && !self.granted.contains(tool)
    }

    /// Decides whether a call may go ahead, asking `approver` if it must.
    ///
    /// The order is deliberate: everything that can be refused without
    /// bothering a person is refused first, so nobody is asked to approve a
    /// call that was never going to run.
    pub fn decide(
        &mut self,
        tool: &str,
        access: Access,
        stack: &str,
        approver: &mut dyn Approver,
    ) -> Decision {
        let refuse = |reason: String| Decision {
            tool: tool.to_string(),
            access,
            stack: stack.to_string(),
            asked: false,
            verdict: Verdict::Refused { reason },
        };

        if let Some(stacks) = &self.stacks
            && !stacks.contains(stack)
        {
            return refuse(format!(
                "this connection may not touch the stack \"{stack}\""
            ));
        }

        if let Some(tools) = &self.tools
            && !tools.contains(tool)
        {
            return refuse(format!("this connection may not use \"{tool}\""));
        }

        if access.writes() && !self.writes {
            return refuse(format!(
                "this connection may only read, and \"{tool}\" would change the stack"
            ));
        }

        if !self.needs_consent(tool, access) {
            return Decision {
                tool: tool.to_string(),
                access,
                stack: stack.to_string(),
                asked: false,
                verdict: Verdict::Allowed,
            };
        }

        let granted = approver.approve(&Approval {
            tool,
            access,
            stack,
        });
        if granted {
            // Asked once per tool, not once per call: a person who has said
            // "yes, you may write fields" should not be asked again on the
            // second field.
            self.granted.insert(tool.to_string());
        }

        Decision {
            tool: tool.to_string(),
            access,
            stack: stack.to_string(),
            asked: true,
            verdict: if granted {
                Verdict::Allowed
            } else {
                Verdict::Refused {
                    reason: format!("the user did not allow \"{tool}\""),
                }
            },
        }
    }

    /// Forgets every consent given so far, so the next call asks again.
    pub fn revoke(&mut self) {
        self.granted.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records what it was asked, so a test can assert on the question and
    /// not merely on the answer.
    struct Spy {
        asked: Vec<String>,
        answer: bool,
    }

    impl Approver for Spy {
        fn approve(&mut self, request: &Approval<'_>) -> bool {
            self.asked
                .push(format!("{} on {}", request.tool, request.stack));
            self.answer
        }
    }

    #[test]
    fn the_default_policy_reads_and_refuses_to_write() {
        let mut policy = Policy::new();

        assert!(
            policy
                .decide("read_field", Access::Read, "Todo", &mut DenyAll)
                .verdict
                .is_allowed()
        );

        let refused = policy.decide("write_field", Access::Write, "Todo", &mut DenyAll);
        assert!(!refused.verdict.is_allowed());
        assert!(
            !refused.asked,
            "nobody should be asked about a call that cannot run anyway"
        );
    }

    #[test]
    fn a_policy_written_for_one_stack_refuses_another() {
        let mut policy = Policy::trusted().for_stacks(["Todo"]);

        assert!(
            policy
                .decide("list_cards", Access::Read, "Todo", &mut DenyAll)
                .verdict
                .is_allowed()
        );

        let refused = policy.decide("list_cards", Access::Read, "Payroll", &mut AllowAll);
        assert_eq!(
            refused.verdict,
            Verdict::Refused {
                reason: "this connection may not touch the stack \"Payroll\"".to_string()
            }
        );
    }

    #[test]
    fn a_tool_outside_the_list_is_refused_even_when_it_only_reads() {
        let mut policy = Policy::trusted().only(["current_card"]);

        assert!(
            policy
                .decide("current_card", Access::Read, "Todo", &mut DenyAll)
                .verdict
                .is_allowed()
        );
        assert!(
            !policy
                .decide("list_cards", Access::Read, "Todo", &mut DenyAll)
                .verdict
                .is_allowed()
        );
    }

    #[test]
    fn a_person_is_asked_once_per_tool_and_not_once_per_call() {
        let mut policy = Policy::supervised();
        let mut spy = Spy {
            asked: Vec::new(),
            answer: true,
        };

        let first = policy.decide("write_field", Access::Write, "Todo", &mut spy);
        let second = policy.decide("write_field", Access::Write, "Todo", &mut spy);

        assert!(first.asked, "the first call has to ask");
        assert!(!second.asked, "the second must not");
        assert!(second.verdict.is_allowed());
        assert_eq!(spy.asked, vec!["write_field on Todo"]);
    }

    #[test]
    fn reading_is_never_queried_when_only_writes_need_consent() {
        let mut policy = Policy::supervised();
        let mut spy = Spy {
            asked: Vec::new(),
            answer: false,
        };

        assert!(
            policy
                .decide("read_field", Access::Read, "Todo", &mut spy)
                .verdict
                .is_allowed()
        );
        assert!(spy.asked.is_empty());
    }

    #[test]
    fn refusing_consent_records_that_a_person_refused_it() {
        let mut policy = Policy::supervised();

        let decision = policy.decide("write_field", Access::Write, "Todo", &mut DenyAll);

        assert!(
            decision.asked,
            "the record must show a person was consulted"
        );
        assert_eq!(
            decision.verdict,
            Verdict::Refused {
                reason: "the user did not allow \"write_field\"".to_string()
            }
        );
    }

    #[test]
    fn revoking_makes_the_next_call_ask_again() {
        let mut policy = Policy::supervised();
        let mut spy = Spy {
            asked: Vec::new(),
            answer: true,
        };

        policy.decide("create_card", Access::Write, "Todo", &mut spy);
        policy.revoke();
        policy.decide("create_card", Access::Write, "Todo", &mut spy);

        assert_eq!(spy.asked.len(), 2);
    }

    #[test]
    fn asking_before_everything_covers_reads_too() {
        let mut policy = Policy::trusted().asking(Consent::BeforeEverything);
        let mut spy = Spy {
            asked: Vec::new(),
            answer: true,
        };

        policy.decide("list_cards", Access::Read, "Todo", &mut spy);

        assert_eq!(spy.asked, vec!["list_cards on Todo"]);
    }
}
