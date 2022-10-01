import sys
import os

from evtx import PyEvtxParser

def main():
    # first parameter is the path to the evtx file
    evtx_file = os.path.abspath(os.path.expanduser(sys.argv[1]))
    parser = PyEvtxParser(evtx_file, number_of_threads=0)
    try:
        for record in parser.records():
            print(f'Event Record ID: {record["event_record_id"]}')
            print(f'Event Timestamp: {record["timestamp"]}')
            print(record['data'])
            print(f'------------------------------------------')
    except RuntimeError as e:
        print(f'Error: {e}')
        exit(0)

if __name__ == '__main__':
    main()
