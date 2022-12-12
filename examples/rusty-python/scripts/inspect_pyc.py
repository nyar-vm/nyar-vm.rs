import marshal
import sys

def inspect_pyc(filename):
    with open(filename, 'rb') as f:
        magic = f.read(4)
        bitfield = f.read(4)
        timestamp = f.read(4)
        size = f.read(4)
        print(f"Magic: {magic.hex()}")
        print(f"Bitfield: {bitfield.hex()}")
        print(f"Timestamp: {timestamp.hex()}")
        print(f"Size: {size.hex()}")
        try:
            code = marshal.load(f)
            print("Successfully loaded code object")
            print(f"Name: {code.co_name}")
            print(f"Filename: {code.co_filename}")
            print(f"Argcount: {code.co_argcount}")
            print(f"Nlocals: {getattr(code, 'co_nlocals', 'N/A')}")
            print(f"Stacksize: {code.co_stacksize}")
            print(f"Flags: {code.co_flags}")
            print(f"Code: {code.co_code.hex()}")
            print(f"Consts: {code.co_consts}")
            print(f"Names: {code.co_names}")
            print(f"Localsplusnames: {getattr(code, 'co_localsplusnames', 'N/A')}")
            print(f"Varnames: {code.co_varnames}")
            print(f"Freevars: {code.co_freevars}")
            print(f"Cellvars: {code.co_cellvars}")
            print(f"Linetable: {code.co_linetable.hex()}")
            print(f"Exceptiontable: {getattr(code, 'co_exceptiontable', b'').hex()}")
        except Exception as e:
            print(f"Error loading code object: {e}")

if __name__ == "__main__":
    if len(sys.argv) > 1:
        inspect_pyc(sys.argv[1])
    else:
        print("Usage: python inspect_pyc.py <filename.pyc>")
