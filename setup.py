import os
import sys

from setuptools import setup
from setuptools_rust import RustExtension


def get_py_version_cfgs():
    # For now each Cfg Py_3_X flag is interpreted as "at least 3.X"
    version = sys.version_info[0:2]
    py3_min = 5
    out_cfg = []
    for minor in range(py3_min, version[1] + 1):
        out_cfg.append("--cfg=Py_3_%d" % minor)

    return out_cfg


install_requires = []

setup(
    name="pyevtx-rs",
    version="0.1.0",
    classifiers=[
        "License :: OSI Approved :: MIT License",
        "Development Status :: 3 - Alpha",
        "Intended Audience :: Developers",
        "Programming Language :: Python",
        "Programming Language :: Rust",
        "Operating System :: POSIX",
        "Operating System :: MacOS :: MacOS X",
    ],
    packages=["pyevtx_rs"],
    rust_extensions=[
        RustExtension(
            "pyevtx_rs.evtx_parser",
            "Cargo.toml",
            debug=os.getenv("PYEVTX_DEBUG", False),
            rustc_flags=get_py_version_cfgs(),
        ),
    ],
    install_requires=install_requires,
    include_package_data=True,
    zip_safe=False,
)