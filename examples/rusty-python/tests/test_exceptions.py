
def test_raise():
    raise Exception("error")

def test_try_except():
    try:
        raise Exception("error")
    except Exception as e:
        print("caught")
    finally:
        print("finally")

def test_assert():
    assert 1 == 1
    assert 1 == 0, "should fail"

def test_with():
    with open("test.txt") as f:
        print(f.read())

test_try_except()
test_assert()
