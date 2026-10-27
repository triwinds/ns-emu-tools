"""Exercise the project's reqwest TLS configuration without changing OS trust.

Run from the repository root: python src-tauri/tests/tls_isolation.py
Requires Cargo, OpenSSL, and Python 3. No third-party Python packages are needed.
Certificates and private keys are temporary; only a JSON report remains in target/.
"""

import http.server
import json
import os
from pathlib import Path
import shutil
import ssl
import subprocess
import tempfile
import threading


PROJECT = Path(__file__).resolve().parents[1]
REPORT = PROJECT / "target" / "tls-isolation-report.json"


def run(command, **kwargs):
    result = subprocess.run(
        command, text=True, encoding="utf-8", errors="replace",
        capture_output=True, cwd=PROJECT, **kwargs
    )
    if result.returncode:
        raise RuntimeError(
            f"Command failed: {command}\n{result.stdout}\n{result.stderr}"
        )
    return result


def build_probe():
    print("Building probe against the project's locked dependencies...", flush=True)
    result = run(
        [
            "cargo", "test", "--offline", "--locked", "--test",
            "tls_isolation_probe", "--no-run", "--message-format=json",
        ],
        timeout=600,
    )
    for line in result.stdout.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (
            item.get("reason") == "compiler-artifact"
            and item.get("target", {}).get("name") == "tls_isolation_probe"
            and item.get("executable")
        ):
            return item["executable"]
    raise RuntimeError("Cargo did not return a probe executable")


def make_ca(openssl, directory, name):
    run(
        [
            openssl, "req", "-config", str(directory / "openssl.cnf"),
            "-x509", "-newkey", "rsa:2048", "-nodes",
            "-sha256", "-days", "1", "-subj", f"/CN=NS Emu TLS Test {name}",
            "-addext", "basicConstraints=critical,CA:TRUE",
            "-addext", "keyUsage=critical,keyCertSign,cRLSign",
            "-keyout", str(directory / f"{name}.key"),
            "-out", str(directory / f"{name}.pem"),
        ],
        timeout=30,
    )


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"tls-isolation-ok"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


def exercise(probe, openssl, directory):
    # Some Windows OpenSSL distributions have no default configuration file.
    (directory / "openssl.cnf").write_text(
        "[req]\ndistinguished_name=dn\n[dn]\n", encoding="ascii"
    )
    make_ca(openssl, directory, "trusted-ca")
    make_ca(openssl, directory, "unrelated-ca")
    run(
        [
            openssl, "req", "-config", str(directory / "openssl.cnf"),
            "-new", "-newkey", "rsa:2048", "-nodes",
            "-subj", "/CN=localhost", "-keyout", str(directory / "server.key"),
            "-out", str(directory / "server.csr"),
        ],
        timeout=30,
    )
    extensions = directory / "server.ext"
    extensions.write_text(
        "basicConstraints=critical,CA:FALSE\n"
        "keyUsage=critical,digitalSignature,keyEncipherment\n"
        "extendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\n",
        encoding="ascii",
    )
    run(
        [
            openssl, "x509", "-req", "-in", str(directory / "server.csr"),
            "-CA", str(directory / "trusted-ca.pem"),
            "-CAkey", str(directory / "trusted-ca.key"), "-set_serial", "2",
            "-days", "1", "-sha256", "-extfile", str(extensions),
            "-out", str(directory / "server.pem"),
        ],
        timeout=30,
    )
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(directory / "server.pem", directory / "server.key")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    port = server.server_port
    cases = [
        ("no_test_ca", "localhost", None, "UnknownIssuer", False),
        ("trusted_test_ca", "localhost", "trusted-ca", "success", False),
        ("unrelated_ca", "localhost", "unrelated-ca", "UnknownIssuer", False),
        ("hostname_mismatch", "127.0.0.1", "trusted-ca", "NotValidForName", False),
        ("previous_config_control", "localhost", "trusted-ca", "UnknownIssuer", True),
    ]
    results = []
    try:
        for name, host, ca, expected, disable_native in cases:
            env = os.environ.copy()
            for key in (
                "SSL_CERT_FILE", "SSL_CERT_DIR",
                "NS_EMU_TLS_TEST_DISABLE_NATIVE_ROOTS",
            ):
                env.pop(key, None)
            if ca:
                env["SSL_CERT_FILE"] = str(directory / f"{ca}.pem")
            if disable_native:
                env["NS_EMU_TLS_TEST_DISABLE_NATIVE_ROOTS"] = "1"
            env["NS_EMU_TLS_TEST_URL"] = f"https://{host}:{port}/"
            env["NS_EMU_TLS_TEST_EXPECT"] = expected
            result = subprocess.run(
                [probe, "--ignored", "--exact", "tls_isolation_probe", "--nocapture"],
                env=env, cwd=PROJECT, capture_output=True, text=True,
                encoding="utf-8", errors="replace", timeout=30,
            )
            passed = result.returncode == 0
            results.append(
                {
                    "case": name, "expected": expected, "passed": passed,
                    "stdout": result.stdout, "stderr": result.stderr,
                }
            )
            print(f"{'PASS' if passed else 'FAIL'} {name}: {expected}", flush=True)
            if not passed:
                print(result.stdout + result.stderr, flush=True)
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)
    return results


def main():
    openssl = shutil.which("openssl")
    if not openssl:
        raise RuntimeError("OpenSSL must be available on PATH")
    probe = build_probe()
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tls-isolation-", dir=REPORT.parent) as tmp:
        results = exercise(probe, openssl, Path(tmp))
    REPORT.write_text(
        json.dumps({"system_trust_store_modified": False, "results": results}, indent=2),
        encoding="utf-8",
    )
    print(f"Report: {REPORT}", flush=True)
    print("Temporary certificates removed; OS certificate stores were not modified.")
    return 0 if all(item["passed"] for item in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
