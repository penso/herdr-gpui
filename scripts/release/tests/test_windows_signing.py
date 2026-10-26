"""Run: python3 -m unittest discover -s scripts/release/tests -v"""
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SCRIPTS = Path(__file__).resolve().parents[1]
SIGNING = load("windows_signing", SCRIPTS / "windows-signing.py")
INSTALLER = load("windows_installer", SCRIPTS / "windows-installer.py")
PACKAGE = load("package_windows", SCRIPTS / "package-windows.py")
VERSION = "20260920.3"
TARGET = "x86_64-pc-windows-msvc"
ROOT = f"Herdr-{VERSION}-{TARGET}"
OPTIONAL = 0x40 + 24
CHECKSUM = OPTIONAL + 64
SECURITY = OPTIONAL + 112 + 4 * 8


def executable(subsystem=2, length=0x203):
    """A PE32+ image with all sixteen data directories, an odd length so
    signing has to pad, and a checksum the signer will rewrite."""
    optional = (struct.pack("<H", 0x20B) + bytes(62) + struct.pack("<I", 0x1234)
                + struct.pack("<H", subsystem) + bytes(38) + struct.pack("<I", 16) + bytes(16 * 8))
    image = b"MZ" + bytes(0x3A) + struct.pack("<I", 0x40) + b"PE\0\0" + bytes(20) + optional
    return image + bytes(range(256)) * (length // 256) + bytes(length % 256)


def sign(data, padding=None, certificate=b"\x30\x82signature!"):
    """`data` as signtool leaves it: padded to 8 bytes, a certificate table
    appended and pointed at, and a new checksum."""
    padding = (-len(data)) % 8 if padding is None else padding
    signed = bytearray(data + bytes(padding))
    struct.pack_into("<II", signed, SECURITY, len(signed), len(certificate))
    struct.pack_into("<I", signed, CHECKSUM, 0xBEEF)
    return bytes(signed + certificate)


class SignatureTests(unittest.TestCase):
    def test_accepts_only_an_appended_signature(self):
        app = executable()
        SIGNING.without_signature(sign(app), app)
        aligned = executable(length=0x200)
        SIGNING.without_signature(sign(aligned), aligned)

    def test_rejects_any_other_change(self):
        app = executable()
        changed = bytearray(sign(app))
        changed[0x150] ^= 1
        subsystem = bytearray(sign(app))
        struct.pack_into("<H", subsystem, OPTIONAL + 68, 3)
        unaligned = bytearray(sign(app, padding=0))
        dirty_padding = bytearray(sign(app))
        dirty_padding[len(app)] = 1
        short_table = bytearray(sign(app))
        struct.pack_into("<I", short_table, SECURITY + 4, 4)
        cases = {
            "unsigned": app,
            "changed contents": bytes(changed),
            # Patching the header after signing would break the signature.
            "changed subsystem": bytes(subsystem),
            "unaligned table": bytes(unaligned),
            "data in the padding": bytes(dirty_padding),
            "table not at the end": bytes(short_table),
            "appended data": sign(app) + b"x",
            "different image": sign(executable(length=0x303)),
            "not an executable": b"ZM" + sign(app)[2:],
        }
        for name, signed in cases.items():
            with self.subTest(name), self.assertRaises(ValueError):
                SIGNING.without_signature(signed, app)
        with self.assertRaises(ValueError):
            SIGNING.without_signature(sign(sign(app)), sign(app))

    def test_rejects_images_without_a_certificate_directory(self):
        app = bytearray(executable())
        struct.pack_into("<I", app, OPTIONAL + 108, 4)
        with self.assertRaises(ValueError):
            SIGNING.certificate_table(bytes(app))
        pe32 = bytearray(executable())
        struct.pack_into("<H", pe32, OPTIONAL, 0x10B)
        # PE32's directories start 16 bytes earlier, where this image has zeros.
        with self.assertRaises(ValueError):
            SIGNING.certificate_table(bytes(pe32))


class RepackTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="herdr-signing-test-")
        self.addCleanup(temp.cleanup)
        self.work = Path(temp.name)
        self.app = executable()
        self.launcher = executable(length=0x303)
        self.acp = executable(length=0x305)
        self.remote_control = executable(length=0x307)
        self.automate = executable(length=0x309)
        self.archive = self.work / f"{ROOT}.zip"
        with zipfile.ZipFile(self.archive, "w") as release:
            PACKAGE.add(release, f"{ROOT}/herdr-gpui.exe", self.app)
            PACKAGE.add(release, f"{ROOT}/herdr-gpui-launcher.exe", self.launcher)
            PACKAGE.add(release, f"{ROOT}/herdr-acp.exe", self.acp)
            PACKAGE.add(release, f"{ROOT}/herdr-remote-control.exe", self.remote_control)
            PACKAGE.add(release, f"{ROOT}/herdr-automate.exe", self.automate)
            PACKAGE.add(release, f"{ROOT}/licenses/LICENSE", b"license")
        self.output = self.work / "signed"
        self.output.mkdir()

    def signed_tree(self):
        root = SIGNING.unpack(self.archive, self.work / "tree")
        self.assertEqual(root, (self.work / "tree" / ROOT).resolve())
        (root / "herdr-gpui.exe").write_bytes(sign(self.app))
        (root / "herdr-gpui-launcher.exe").write_bytes(sign(self.launcher))
        (root / "herdr-acp.exe").write_bytes(sign(self.acp))
        (root / "herdr-remote-control.exe").write_bytes(sign(self.remote_control))
        (root / "herdr-automate.exe").write_bytes(sign(self.automate))
        return root

    def test_repacks_the_signed_tree_in_the_zips_order(self):
        self.signed_tree()
        output = SIGNING.repack(self.archive, self.work / "tree", self.output)
        self.assertEqual(output, self.output.resolve() / self.archive.name)
        with zipfile.ZipFile(self.archive) as unsigned, zipfile.ZipFile(output) as signed:
            self.assertEqual(signed.namelist(), unsigned.namelist())
            self.assertEqual([i.date_time for i in signed.infolist()], [PACKAGE.TIMESTAMP] * len(unsigned.infolist()))
            self.assertEqual(signed.read(f"{ROOT}/herdr-gpui.exe"), sign(self.app))
            self.assertEqual(signed.read(f"{ROOT}/herdr-gpui-launcher.exe"), sign(self.launcher))
            self.assertEqual(signed.read(f"{ROOT}/herdr-acp.exe"), sign(self.acp))
            self.assertEqual(signed.read(f"{ROOT}/herdr-remote-control.exe"), sign(self.remote_control))
            self.assertEqual(signed.read(f"{ROOT}/herdr-automate.exe"), sign(self.automate))
            self.assertEqual(signed.read(f"{ROOT}/licenses/LICENSE"), b"license")
        with self.assertRaisesRegex(ValueError, "Output already exists"):
            SIGNING.repack(self.archive, self.work / "tree", self.output)

    def test_refuses_a_tree_that_is_not_the_signed_zip(self):
        root = self.signed_tree()
        for name, change, undo in [
            ("unsigned launcher", lambda: (root / "herdr-gpui-launcher.exe").write_bytes(self.launcher),
             lambda: (root / "herdr-gpui-launcher.exe").write_bytes(sign(self.launcher))),
            ("unsigned herdr-acp", lambda: (root / "herdr-acp.exe").write_bytes(self.acp),
             lambda: (root / "herdr-acp.exe").write_bytes(sign(self.acp))),
            ("unsigned herdr-remote-control", lambda: (root / "herdr-remote-control.exe").write_bytes(self.remote_control),
             lambda: (root / "herdr-remote-control.exe").write_bytes(sign(self.remote_control))),
            ("unsigned herdr-automate", lambda: (root / "herdr-automate.exe").write_bytes(self.automate),
             lambda: (root / "herdr-automate.exe").write_bytes(sign(self.automate))),
            ("changed license", lambda: (root / "licenses/LICENSE").write_bytes(b"other"),
             lambda: (root / "licenses/LICENSE").write_bytes(b"license")),
            ("extra file", lambda: (root / "extra.dll").write_bytes(b"x"),
             lambda: (root / "extra.dll").unlink()),
        ]:
            with self.subTest(name):
                change()
                with self.assertRaises(ValueError):
                    SIGNING.repack(self.archive, self.work / "tree", self.output)
                undo()
        self.assertEqual(list(self.output.iterdir()), [])

    def test_unpack_refuses_an_existing_directory_or_a_foreign_zip(self):
        (self.work / "tree").mkdir()
        with self.assertRaisesRegex(ValueError, "already exists"):
            SIGNING.unpack(self.archive, self.work / "tree")
        foreign = self.work / f"Herdr-{VERSION}-aarch64-pc-windows-msvc.zip"
        self.archive.rename(foreign)
        with self.assertRaisesRegex(ValueError, "outside its root"):
            SIGNING.unpack(foreign, self.work / "other")


class InstallerSigningTests(unittest.TestCase):
    def test_only_a_signed_build_passes_the_sign_tool(self):
        unsigned = INSTALLER.iscc_arguments(VERSION, TARGET, "src", "out")
        self.assertFalse([argument for argument in unsigned if argument.startswith(("/S", "/DSignTool"))])
        signed = INSTALLER.iscc_arguments(VERSION, TARGET, "src", "out", sign=True)
        self.assertEqual(signed[:len(unsigned) - 1], unsigned[:-1])
        self.assertEqual(signed[-1], unsigned[-1])
        tool = next(argument for argument in signed if argument.startswith("/Sherdr="))
        self.assertIn("/DSignTool=herdr", signed)
        self.assertTrue(tool.endswith(f"-File $q{INSTALLER.SIGN_SCRIPT}$q $f"))
        self.assertNotIn('"', tool)
        self.assertTrue(INSTALLER.SIGN_SCRIPT.is_file())

    def test_the_inno_script_signs_only_when_given_a_sign_tool(self):
        script = INSTALLER.SCRIPT.read_text()
        self.assertIn("#ifdef SignTool\nSignTool={#SignTool}\nSignedUninstaller=yes\n#endif\n", script)
        self.assertEqual(script.count("\nSignTool="), 1)

    def test_the_sign_tool_timestamps_and_uses_only_the_cli_login(self):
        script = INSTALLER.SIGN_SCRIPT.read_text()
        self.assertIn("TimestampRfc3161                    = 'http://timestamp.acs.microsoft.com'", script)
        self.assertIn("ExcludeAzureCliCredential           = $false", script)
        self.assertEqual(script.count("= $true"), 9)
        self.assertIn("Get-AuthenticodeSignature -LiteralPath $Path", script)


if __name__ == "__main__":
    unittest.main()
