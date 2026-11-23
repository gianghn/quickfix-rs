Feature: A broker port listens on one network address

  The trade gateway can limit a broker port to one address of its host, so brokers
  reach it only through the network meant for them.

  Rule: A broker reaches its session through the address the port listens on

    Scenario: Broker logs on through the listening address
      Given a broker port that listens only on the loopback address
      When the broker connects through the loopback address
      Then the broker's session is logged on

  Rule: A connection through any other address of the host is refused

    Scenario: Connection through another address never reaches the session
      Given a broker port that listens only on the loopback address
      When someone connects through another address of the host
      Then the connection is refused

  Rule: A listening address that is not a numeric IPv4 address stops the engine at start-up

    Scenario: Host name as listening address stops the engine
      Given a broker port whose listening address is a host name
      When the engine starts
      Then it stops with a configuration error that names the listening address setting
