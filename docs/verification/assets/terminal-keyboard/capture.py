import json,os,select,sys,termios,time,tty
from pathlib import Path
saved=termios.tcgetattr(0)
try:
    tty.setraw(0)
    os.write(1,b'\x1b[?1h\x1b[?2004hRAW_READY\r\n')
    Path('capture.ready').write_text('ready')
    captured=bytearray();deadline=time.monotonic()+30
    while not captured.endswith(b'\x1b[201~'):
        left=deadline-time.monotonic()
        if left<=0:raise TimeoutError('native capture deadline')
        readable,_,_=select.select([0],[],[],left)
        if not readable:raise TimeoutError('native capture deadline')
        captured.extend(os.read(0,4096))
    Path('captured.bin').write_bytes(captured)
    os.write(1,b'\r\nCAPTURED '+str(len(captured)).encode()+b' bytes\r\n')
finally:
    os.write(1,b'\x1b[?1l\x1b[?2004l')
    termios.tcsetattr(0,termios.TCSANOW,saved)
