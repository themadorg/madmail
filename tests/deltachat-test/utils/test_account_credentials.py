"""Credential helper tests. No relay and no Delta Chat install required."""

from __future__ import annotations

import sys
import types
import unittest


if "deltachat_rpc_client" not in sys.modules:
    try:
        import deltachat_rpc_client  # noqa: F401
    except ImportError:
        stub = types.ModuleType("deltachat_rpc_client")
        stub.EventType = types.SimpleNamespace()
        sys.modules["deltachat_rpc_client"] = stub

from utils.accounts import account_addr, account_password  # noqa: E402


class _ObjectTransport:
    def __init__(self, addr, password):
        self.addr = addr
        self.password = password


class _Account:
    def __init__(self, transports="missing", config=None):
        self._config = config or {}
        if transports != "missing":
            self.list_transports = lambda: transports

    def get_config(self, key):
        return self._config.get(key)


class AccountCredentialTests(unittest.TestCase):
    def test_dict_transport(self):
        account = _Account([{"addr": "a@example.org", "password": "secret"}])
        self.assertEqual(account_addr(account), "a@example.org")
        self.assertEqual(account_password(account), "secret")

    def test_object_transport(self):
        account = _Account([_ObjectTransport("b@example.org", "pw")])
        self.assertEqual(account_addr(account), "b@example.org")
        self.assertEqual(account_password(account), "pw")

    def test_legacy_config_when_list_transports_is_missing(self):
        account = _Account(config={"addr": "c@example.org", "mail_pw": "old"})
        self.assertEqual(account_addr(account), "c@example.org")
        self.assertEqual(account_password(account), "old")

    def test_legacy_config_when_transport_list_is_empty(self):
        account = _Account([], config={"addr": "d@example.org", "mail_pw": "old"})
        self.assertEqual(account_addr(account), "d@example.org")
        self.assertEqual(account_password(account), "old")

    def test_legacy_password_when_transport_has_no_password(self):
        account = _Account(
            [{"addr": "e@example.org"}],
            config={"mail_pw": "legacy"},
        )
        self.assertEqual(account_addr(account), "e@example.org")
        self.assertEqual(account_password(account), "legacy")

    def test_missing_address_raises(self):
        account = _Account([{}], config={})
        with self.assertRaises(RuntimeError):
            account_addr(account)

    def test_missing_password_raises(self):
        account = _Account([{"addr": "f@example.org"}], config={})
        with self.assertRaises(RuntimeError):
            account_password(account)


if __name__ == "__main__":
    unittest.main()
