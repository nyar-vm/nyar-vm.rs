import marshal
import py_compile
import sys

def inspect_official(filename):
    out_file = 'official.pyc'
    py_compile.compile(filename, out_file)
    with open(out_file, 'rb') as f:
        magic = f.read(4)
        bitfield = f.read(4)
        timestamp = f.read(4)
        size = f.read(4)
        code = marshal.load(f)
        print(f"Magic: {magic.hex()}")
        print(f"Name: {code.co_name}")
        print(f"Code Hex: {code.co_code.hex()}")
        print(f"Consts: {code.co_consts}")
        print(f"Names: {code.co_names}")
        print(f"Varnames: {code.co_varnames}")
        print(f"Linetable: {code.co_linetable.hex()}")
        
if __name__ == "__main__":
    if len(sys.argv) > 1:
        inspect_official(sys.argv[1])
    else:
        print("Usage: python inspect_official.py <filename.py>")
