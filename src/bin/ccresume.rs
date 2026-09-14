fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    claude_resume_fzf::main_with_args(args);
}
