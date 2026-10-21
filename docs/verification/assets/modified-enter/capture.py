import json,os,select,termios,time,tty
from pathlib import Path
saved=termios.tcgetattr(0)
try:
 tty.setraw(0)
 os.write(1,b'READY: capture modified Enter\r\n')
 Path('capture.ready').write_text('ready')
 captured=bytearray(); deadline=time.monotonic()+180
 while not captured.endswith(b'\x04'):
  left=deadline-time.monotonic()
  if left<=0:raise TimeoutError('native key capture deadline')
  readable,_,_=select.select([0],[],[],left)
  if not readable:raise TimeoutError('native key capture deadline')
  captured.extend(os.read(0,4096))
 Path('captured.bin').write_bytes(captured)
 os.write(1,b'\r\nCAPTURED '+str(len(captured)).encode()+b' bytes\r\n')
finally:
 termios.tcsetattr(0,termios.TCSANOW,saved)
