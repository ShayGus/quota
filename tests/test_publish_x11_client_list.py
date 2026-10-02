import ctypes
from pathlib import Path
import runpy
import sys
import unittest
from unittest.mock import patch


class NativeFunction:
    def __init__(self, call):
        self.call = call

    def __call__(self, *arguments):
        return self.call(*arguments)


class FreshXServer:
    def __init__(self):
        self.atoms = {}
        self.properties = {}
        self.synced = False
        self.XOpenDisplay = NativeFunction(lambda _: 1)
        self.XDefaultRootWindow = NativeFunction(lambda _: 2)
        self.XInternAtom = NativeFunction(self.intern_atom)
        self.XChangeProperty = NativeFunction(self.change_property)
        self.XSync = NativeFunction(self.sync)

    def intern_atom(self, _display, name, only_if_exists):
        if name not in self.atoms and not only_if_exists:
            self.atoms[name] = len(self.atoms) + 1
        return self.atoms.get(name, 0)

    def change_property(self, _display, _root, atom, kind, bits, mode, data, count):
        if atom == 0 or kind == 0:
            raise ValueError("BadAtom")
        values = ctypes.cast(data, ctypes.POINTER(ctypes.c_ulong))
        self.properties[atom] = (kind, bits, mode, [values[i] for i in range(count)])

    def sync(self, _display, _discard):
        self.synced = True


class PublishClientListTest(unittest.TestCase):
    def test_missing_atoms_are_created_and_both_lists_are_published(self):
        server = FreshXServer()
        helper = Path(__file__).resolve().parent.parent / "docs/publish-x11-client-list.py"
        with patch.object(ctypes, "CDLL", return_value=server), patch.object(
            sys, "argv", [str(helper), "0x600010", "0x600020"]
        ):
            with self.assertRaises(SystemExit) as result:
                runpy.run_path(str(helper), run_name="__main__")
        self.assertEqual(result.exception.code, 0)
        self.assertTrue(server.synced)
        for name in (b"_NET_CLIENT_LIST", b"_NET_CLIENT_LIST_STACKING"):
            self.assertEqual(
                server.properties[server.atoms[name]],
                (server.atoms[b"CARDINAL"], 32, 0, [0x600010, 0x600020]),
            )


if __name__ == "__main__":
    unittest.main()
