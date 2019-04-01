from pyevtx_rs.evtx_parser import PyEvtxParser

def test_it_works():
    parser = PyEvtxParser("/Users/omerba/Workspace/evtx-rs/samples/security.evtx")
    for record in parser:
        print(record)