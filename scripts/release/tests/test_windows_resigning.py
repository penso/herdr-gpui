"""Signature replacement for the pinned, already-signed Windows companion."""
import struct
import unittest
import zipfile

import test_windows_signing as fixtures
from test_windows_signing import (
    OPTIONAL, PACKAGE, ROOT, SECURITY, SIGNING, executable, sign,
)


class ResigningTests(unittest.TestCase):
    def setUp(self):
        self.original = sign(executable(), certificate=b"upstream certificate")
        self.offset = SIGNING.certificate_table(self.original)[2]
        self.resigned = sign(self.original[:self.offset], certificate=b"new certificate")

    def test_replaces_a_trailing_certificate_with_a_different_size(self):
        SIGNING.without_signature(self.resigned, self.original)
        larger = sign(self.original[:self.offset], certificate=b"new certificate" * 10)
        SIGNING.without_signature(larger, self.original)

    def test_rejects_changes_outside_the_certificate(self):
        for position in (0x150, OPTIONAL + 68, self.offset - 1):
            with self.subTest(position=position):
                changed = bytearray(self.resigned)
                changed[position] ^= 1
                with self.assertRaises(ValueError):
                    SIGNING.without_signature(changed, self.original)

    def test_rejects_missing_moved_or_non_trailing_new_table(self):
        for changed in (
            executable(), self.resigned + b"extra",
            sign(self.original[:self.offset - 8]),
            sign(self.original[:self.offset] + bytes(8)),
            sign(self.original),
        ):
            with self.subTest(length=len(changed)), self.assertRaises(ValueError):
                SIGNING.without_signature(changed, self.original)

    def test_rejects_malformed_original_certificate_ranges(self):
        for offset, size in (
            (0, 8), (self.offset, 0), (self.offset + 1, 19),
            (self.offset, 19), (self.offset, 21),
            (SECURITY, len(self.original) - SECURITY),
            (0xfffffff8, 8),
        ):
            with self.subTest(offset=offset, size=size):
                original = bytearray(self.original)
                struct.pack_into("<II", original, SECURITY, offset, size)
                with self.assertRaises(ValueError):
                    SIGNING.without_signature(self.resigned, original)


class PresignedCompanionRepackTests(unittest.TestCase):
    def setUp(self):
        fixtures.RepackTests.setUp(self)
        self.unsigned_core = self.core
        self.core = sign(self.core, certificate=b"upstream certificate")
        with zipfile.ZipFile(self.archive) as archive:
            entries = [(name, archive.read(name)) for name in archive.namelist()]
        with zipfile.ZipFile(self.archive, "w") as archive:
            for name, data in entries:
                PACKAGE.add(archive, name, self.core if name == f"{ROOT}/herdr-core.exe" else data)

    def signed_tree(self):
        root = fixtures.RepackTests.signed_tree(self)
        (root / "herdr-core.exe").write_bytes(sign(self.unsigned_core))
        return root

    def test_repacks_the_signed_tree_in_the_zips_order(self):
        self.signed_tree()
        output = SIGNING.repack(self.archive, self.work / "tree", self.output)
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(archive.read(f"{ROOT}/herdr-core.exe"), sign(self.unsigned_core))

    def test_repack_names_the_companion_when_its_contents_change(self):
        root = self.signed_tree()
        changed = bytearray(sign(self.unsigned_core))
        changed[0x150] ^= 1
        (root / "herdr-core.exe").write_bytes(changed)
        with self.assertRaisesRegex(ValueError, "herdr-core.exe: Signing changed"):
            SIGNING.repack(self.archive, self.work / "tree", self.output)
        self.assertEqual(list(self.output.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
