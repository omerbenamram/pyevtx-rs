"""Fail unless the installed evtx is the wheel in dist.

The PGO jobs install an instrumented wheel to collect a profile, then build the
optimized wheel of the same version. pip leaves an installed package of that
version in place unless told to reinstall it, and the tests would then run
against the instrumented build instead of the wheel that ships.
"""

import importlib.util
import zipfile
from pathlib import Path


wheels = list(Path("dist").glob("*.whl"))
if len(wheels) != 1:
    raise SystemExit(f"Expected one wheel in dist, found {[w.name for w in wheels]}")
wheel = wheels[0]

spec = importlib.util.find_spec("evtx")
if spec is None or not spec.submodule_search_locations:
    raise SystemExit("evtx is not installed")
installed = Path(next(iter(spec.submodule_search_locations)))

with zipfile.ZipFile(wheel) as archive:
    names = [name for name in archive.namelist() if name.startswith("evtx/")]
    if not any(Path(name).stem.startswith("_native") for name in names):
        raise SystemExit(f"{wheel.name} has no evtx/_native extension module")
    for name in names:
        path = installed.joinpath(*name.split("/")[1:])
        if not path.is_file() or path.read_bytes() != archive.read(name):
            raise SystemExit(
                f"{path} is not the {name} of {wheel.name}: "
                "the installed evtx is another build of this version"
            )
print(f"The installed evtx at {installed} is {wheel.name}")
