import math
from os import path

x = 10

def test_global():
    global x
    x = 20

def test_nonlocal():
    y = 30
    def inner():
        nonlocal y
        y = 40
    inner()
    return y

test_global()
res = test_nonlocal()
print(x)
print(res)
