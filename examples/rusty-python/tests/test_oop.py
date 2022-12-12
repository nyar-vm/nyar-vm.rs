class Person:
    def initiate(self, name):
        self.name = name
    def say(self):
        print(self.name)

p = Person("Alice")
p.say()
print(p.name)
