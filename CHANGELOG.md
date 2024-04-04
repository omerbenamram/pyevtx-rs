# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.4]

- Update PyO3 Version to 0.21.
- Update evtx library to 0.8.2.

## [0.8.3]

- Preserve record order during chunk iteration.

## [0.8.2]

- Update PyO3 version

## [0.8.1]

- Wheels are now published using abi3 tag, which means a single wheel can be used for all interpreters.
- Added profile guided optimization for the wheels - should yield a 10% improvement in performance.

## [0.8.0]

- Updated release to match `evtx 0.8.0`
- Added m1 builds

## [0.7.4]

- The parser's settings are now passed correctly to the iterator.

## [0.7.3] - 2022-02-13

- Publish python 3.10 wheels.

## [0.7.2] - 2020-07-13

- Bump to evtx library 0.7.2, and update PyO3
- Publish python 3.9 wheels.

## [0.6.11] - 2020-07-13

Bump to evtx library 0.6.8, and update PyO3.

## [0.6.10] - 2020-07-13

Updated wheels to rust stable, support python 3.8.

## [0.6.5] - 2020-01-14

Updated release to match `evtx 0.6.5` - also opts out of `evtx_dump`
dependencies.

## [0.6.3] - 2019-12-17

Updated release to match `evtx 0.6.3`

## [0.6.2] - 2019-12-17

Updated release to match `evtx 0.6.2`

## [0.4.0] - 2019-06-02

### Added

- PyEvtxParser now supports settings number of worker threads, and underlying
  ansi strings codec.

## [0.3.2] - 2019-05-20

### Added

- PyEvtxParser now supports file-like objects.
