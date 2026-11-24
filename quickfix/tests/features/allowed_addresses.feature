Feature: A broker session logs on only from the addresses it lists

  The trade gateway can list, per broker session, the addresses that broker connects from,
  so a host that reaches the port cannot log on as that broker with its names alone.

  Rule: A broker logs on from an address its session lists

    Scenario: Broker logs on from a listed address
      Given a broker session that may log on from the loopback address
      When the broker connects through the loopback address
      Then the broker's session is logged on

  Rule: A broker from an address its session does not list is dropped

    Scenario: Broker from an unlisted address is dropped
      Given a broker session that may log on only from another host
      When the broker connects through the loopback address
      Then the broker's session is not logged on

  Rule: An address list that is not a list of IPv4 addresses stops the engine at start-up

    Scenario: Host name in the address list stops the engine
      Given a broker session whose address list holds a host name
      When the engine starts
      Then it stops with a configuration error that names the address list setting
