# Required Cargo features:
#   lidi-send:    command-line, from-tcp, send-mmsg
#   lidi-receive: command-line, to-tcp, receive-mmsg
#   lidi-clients: tcp, inotify
#
# The inotify feature enables --watch in lidi-dir-send. All scenarios use it.
#
Feature: Check lidi-dir-send is not sending ignored files

  Scenario: Copy a dot file with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with non-recursive dynamic watch and ignore dot files
    When we copy a file .A of size 1KB
    Then lidi-file-receive no file .A in 5 seconds
    Then file .A is in source directory 

  Scenario: Move a dot file with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with non-recursive dynamic watch and ignore dot files
    When we move a file .A of size 1KB
    Then lidi-file-receive no file .A in 5 seconds
    Then file .A is in source directory 

  Scenario: Move a dot directory with dynamic lidi-dir-send
    # In dynamic mode, given the hierarchy:
    #    DIR
    #    ├── .A        (dir)
    #    │   └── AA    (file)
    #    ├── B         (dir)
    #    │   └── BB    (dir)
    #    └── .C        (file)
    # no file should be sent (AA is behind a dot directory, .C is a dot file, B/BB is an empty dir).
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with recursive dynamic watch and ignore dot files
    When we move a directory DIR of size 1KB in the input directory
    Then lidi-file-receive no dir DIR in 2 seconds
    And DIR/.C exists in input directory
    And DIR/.A/AA exists in input directory

  Scenario: Move a dot directory with static lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with non-recursive static watch and ignore dot files
    When we move a directory .A of size 1KB in the input directory
    Then lidi-file-receive no dir .A in 2 seconds
    And .A exists in input directory

  Scenario: Move a directory containing a dot file with lidi-dir-send
    # Directories should be sent without filtering their content.
    Given lidi-file-receive uses a temporary directory
    And lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with non-recursive static watch and ignore dot files
    When we move a directory DIR of size 1KB in the input directory
    Then lidi-file-receive dir DIR in 5 seconds

  Scenario: Ignore a dot directory that already exists before lidi-dir-send's initial recursive scan
    # --ignore must be applied during the *initial* directory scan
    # performed by send_dir(), not only to files that show up later
    # through inotify.
    Given lidi is started with max throughput of 100mbit
    And a directory DIR with a dot subdirectory already exists in send directory
    And lidi-dir-send is started with recursive dynamic watch and ignore dot files
    Then lidi-file-receive no dir DIR in 5 seconds
    And DIR/.A/AA exists in input directory
    And DIR/.C exists in input directory
