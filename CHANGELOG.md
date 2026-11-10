# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Parsing in a forked child process hung forever once the parent had created a
  parser: a `multiprocessing` pool with the fork start method (the default on
  Linux before Python 3.14) never returned. 0.15.0 parses chunks on rayon's
  global thread pool, whose threads don't exist in the child. Chunks are now
  parsed on a pool of pyevtx's own, which a forked child replaces with a new
  one, and which is started by the first parse instead of by creating a parser.
  0.14.1 and earlier are not affected. An iterator that was already going when
  the process forked raises one `RuntimeError` in the child for the chunks that
  were being parsed, instead of waiting for them forever, and then goes on.
  The pool still has `RAYON_NUM_THREADS` threads when that is set.
