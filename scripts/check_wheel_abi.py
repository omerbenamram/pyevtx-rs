"""Reject wheels built for the wrong interpreter before uploading artifacts."""

import sys
from pathlib import Path


expected = {"3.10": ("cp310", "abi3"), "3.14t": ("cp314", "cp314t")}[sys.argv[1]]
wheels = list(Path("dist").glob("*.whl"))
if not wheels:
    raise SystemExit("No wheels found in dist")
for wheel in wheels:
    python_tag, abi_tag, _ = wheel.name.rsplit("-", 3)[-3:]
    if (python_tag, abi_tag) != expected:
        raise SystemExit(f"Wrong ABI: {wheel.name}; expected {expected}")
    print(f"Verified {wheel.name}")
