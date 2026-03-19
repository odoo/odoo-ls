class MyClass:
    pass

class MyContextManager:
    def __enter__(self) -> MyClass:
        pass

    def __exit__(self, exc_type, exc_val, exc_tb):
        pass

def get_my_class() -> MyClass:
    pass

result = get_my_class()
result

with MyContextManager() as ctx:
    ctx
