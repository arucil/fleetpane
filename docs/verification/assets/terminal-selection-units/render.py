import pathlib,time
print("\x1b[2J\x1b[H[src/main.rs] --flag=中文 e\u0301  ",flush=True)
print("second row: alpha \x1b[31mRED\x1b[m omega",flush=True)
print("wrapped="+"0123456789"*8,flush=True)
print("last visible line",flush=True)
while not pathlib.Path("release-output").exists(): time.sleep(.05)
for i in range(60): print(f"BACKGROUND-{i:02}",flush=True)
print("AFTER_OUTPUT",flush=True)
