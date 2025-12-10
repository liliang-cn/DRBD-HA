#[cfg(test)]
pub mod tests {
    use crate::CommandOutput;
    use std::collections::VecDeque;
    use std::sync::{Mutex, OnceLock};

    static MOCK_COMMAND_OUTPUTS: OnceLock<Mutex<VecDeque<CommandOutput>>> = OnceLock::new();
    static RECORDED_COMMANDS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

    fn get_mock_queue() -> &'static Mutex<VecDeque<CommandOutput>> {
        MOCK_COMMAND_OUTPUTS.get_or_init(|| Mutex::new(VecDeque::new()))
    }

    fn get_recorded_commands() -> &'static Mutex<Vec<String>> {
        RECORDED_COMMANDS.get_or_init(|| Mutex::new(Vec::new()))
    }

    pub fn clear_mocks() {
        if let Ok(mut queue) = get_mock_queue().lock() {
            queue.clear();
        }
        if let Ok(mut cmds) = get_recorded_commands().lock() {
            cmds.clear();
        }
    }

    pub fn push_mock_output(output: CommandOutput) {
        get_mock_queue().lock().unwrap().push_back(output);
    }

    pub fn get_next_mock_output() -> Option<CommandOutput> {
        get_mock_queue().lock().unwrap().pop_front()
    }

    pub fn record_command(cmd: String) {
        get_recorded_commands().lock().unwrap().push(cmd);
    }

    pub fn get_recorded_commands_list() -> Vec<String> {
        get_recorded_commands().lock().unwrap().clone()
    }
}
