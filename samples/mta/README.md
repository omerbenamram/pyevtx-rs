# `.MTA` sample

`classic.evtx` and `LocaleMetaData/classic_1033.MTA` are copied from evtx's
[`samples/mta`](https://github.com/omerbenamram/evtx/tree/master/samples/mta), where its README
says how they were made: four `Application` events from two classic event sources, exported
with display information on a GitHub Actions Windows runner, and what Windows renders from the
archive. `tests/test_mta.py` checks the messages against that rendering.
