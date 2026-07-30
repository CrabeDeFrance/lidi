# Required Cargo features:
#   lidi-clients: unix
#
# These scenarios invoke lidi-file-receive directly with --from-unix, without
# starting the full lidi-send/lidi-receive pipeline: they only exercise the
# startup-time handling of a pre-existing path at the Unix socket location.
#
Feature: lidi-file-receive refuses to delete a non-socket file at --from-unix path

  Scenario: Regression test for 14176d0 - refuse to delete a regular file at the Unix socket path
    # Before the fix, lidi-file-receive blindly called remove_file() on any
    # pre-existing path at --from-unix, silently destroying unrelated user
    # files. The fix validates the path is actually a socket before deleting it.
    Given a regular file exists at the Unix socket path
    When lidi-file-receive is started with --from-unix at that path
    Then lidi-file-receive exits with an error mentioning "not a socket"
    And the regular file at the Unix socket path still exists and is unchanged
