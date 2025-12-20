use ssh_cmd::CommandOutput;
use std::cell::RefCell;
use std::collections::VecDeque;

thread_local! {
    pub static MOCK_OUTPUTS: RefCell<VecDeque<CommandOutput>> = RefCell::new(VecDeque::new());
    pub static RECORDED_COMMANDS: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

#[allow(dead_code)]
pub fn push_mock_output(output: CommandOutput) {
    MOCK_OUTPUTS.with(|m| m.borrow_mut().push_back(output));
}

#[allow(dead_code)]
pub fn get_next_mock_output() -> Option<CommandOutput> {
    MOCK_OUTPUTS.with(|m| m.borrow_mut().pop_front())
}

#[allow(dead_code)]
pub fn record_command(command: String) {
    RECORDED_COMMANDS.with(|r| r.borrow_mut().push(command));
}

#[allow(dead_code)]
pub fn get_recorded_commands_list() -> Vec<String> {
    RECORDED_COMMANDS.with(|r| r.borrow().clone())
}

#[allow(dead_code)]
pub fn clear_mocks() {
    MOCK_OUTPUTS.with(|m| m.borrow_mut().clear());
    RECORDED_COMMANDS.with(|r| r.borrow_mut().clear());
}

pub mod tests {
    pub use super::*;
}