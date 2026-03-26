mod support;

#[test]
fn support_builds_attach_and_server_commands_with_isolated_env() {
    let env = support::isolated_humu_home();
    let server = support::humu_server_command(&env);
    let attach = support::humu_attach_command(&env, "default");

    assert_eq!(server.get_program(), support::humu_binary().as_os_str());
    assert_eq!(
        server.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("server")]
    );
    assert_eq!(attach.get_program(), support::humu_binary().as_os_str());
    assert_eq!(
        attach.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("attach"), std::ffi::OsStr::new("default")]
    );
}
