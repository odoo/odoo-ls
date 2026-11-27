from .file_a import ClassA


class ClassC(ClassA):

    def method_c(self):
        return self.method_a()
