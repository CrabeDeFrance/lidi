Feature: Check diode-send-dir is still working after a diode-send restart

  Scenario: Send one file then restart sender finally send a file
    Given diode with send-dir is started
    When we copy a file A of size 10KB
    And diode-file-receive file A in 5 seconds
    And diode-send is restarted
    And we copy a file B of size 10KB
    Then diode-file-receive file B in 5 seconds

