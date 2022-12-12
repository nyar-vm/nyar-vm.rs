
import marshal
import types

code_str = "x = 1"
code_obj = compile(code_str, "test.py", "exec")

print(f"Code object fields:")
for field in dir(code_obj):
    if field.startswith("co_"):
        print(f"{field}: {getattr(code_obj, field)}")

marshal_data = marshal.dumps(code_obj)
print(f"\nMarshal data (hex):")
print(marshal_data.hex())

# Try to decode it step by step
print("\nMarshal data breakdown:")
i = 0
while i < len(marshal_data):
    code = marshal_data[i]
    char = chr(code) if 32 <= code <= 126 else "."
    print(f"{i:03d} (0x{i:02x}): {code:02x} ({char})")
    i += 1
