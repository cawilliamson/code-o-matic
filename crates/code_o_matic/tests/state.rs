#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
use code_o_matic::state::{Event, State};

#[test]
fn state_transitions_legal() {
    // when: a session in the awaiting_user state receives a user message
    let next = State::AwaitingUser.next(Event::UserMessage);
    // then: the session transitions to the thinking state
    assert_eq!(next, State::Thinking);

    // when: a session in the thinking state receives tool calls
    let next = State::Thinking.next(Event::ToolCalls);
    // then: the session transitions to the tool_running state
    assert_eq!(next, State::ToolRunning);

    // when: a session in the tool_running state collects all tool results
    let next = State::ToolRunning.next(Event::ToolsDone);
    // then: the session transitions back to the thinking state
    assert_eq!(next, State::Thinking);

    // when: a session in the thinking state produces text without further tool calls
    let next = State::Thinking.next(Event::FinalResponse);
    // then: the session transitions to the responding state
    assert_eq!(next, State::Responding);

    // when: a session in the responding state delivers the response
    let next = State::Responding.next(Event::TurnComplete);
    // then: the session transitions to the awaiting_user state
    assert_eq!(next, State::AwaitingUser);
}

#[test]
fn illegal_transitions_preserve_state() {
    // when: an awaiting_user session receives a non-user event
    let next = State::AwaitingUser.next(Event::ToolCalls);
    // then: the session remains in the awaiting_user state
    assert_eq!(next, State::AwaitingUser);

    // when: a tool_running session receives a user message
    let next = State::ToolRunning.next(Event::UserMessage);
    // then: the session remains in the tool_running state
    assert_eq!(next, State::ToolRunning);

    // when: a responding session receives a final response event
    let next = State::Responding.next(Event::FinalResponse);
    // then: the session remains in the responding state
    assert_eq!(next, State::Responding);
}
