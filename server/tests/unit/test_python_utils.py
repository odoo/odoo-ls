import ast
import pytest


#####################
# Assignments Tests #
#####################

from server.python_utils import PythonUtils

def parse_assign(expr: str):
   return ast.parse(expr).body[0]

class TestAssign:
   def test_simple_assign(self):
      expr = "a = 1"
      node = parse_assign(expr)
      res = PythonUtils.unpack_assign(node.target, node.value)

      target = list(res.keys())[0].id
      value = list(res.values())[0].value

      assert target == 'a'
      assert value == 1
   
   def test_multi_assign(self):
      expr = "a = b = 1"
      node = parse_assign(expr)
      res = PythonUtils.unpack_assign(node.target, node.value)

      for k, v in res.items():
         if k.id == 'a':
            assert v.value == 1

         elif k.id == 'b':
            assert v.value == 1

   def test_unpacking(self):
      expr = "a,b = 1,2"
      node = parse_assign(expr)
      res = PythonUtils.unpack_assign(node.target, node.value)

      for k, v in res.items():
         if k.id == 'a':
            assert v.value == 1

         elif k.id == 'b':
            assert v.value == 2
   
   def test_nested_unpacking(self):
      expr = "a,b,(c,(d,e)),f = 1,2,(3,(4,5)),6"
      node = parse_assign(expr)
      res = PythonUtils.unpack_assign(node.target, node.value)

      for k, v in res.items():
         if k.id == 'a':
            assert v.value == 1

         elif k.id == 'b':
            assert v.value == 2

         elif k.id == 'c':
            assert v.value == 3

         elif k.id == 'd':
            assert v.value == 4

         elif k.id == 'e':
            assert v.value == 5
         
         elif k.id == 'f':
            assert v.value == 6
      
   def test_invalid_unpack(self):
      expr = "a,b = 1"
      node = parse_assign(expr)
      res = PythonUtils.unpack_assign(node.target, node.value)

      assert not res

   def test_invalid_nested_unpack(self):
      expr = "a,(b,c,(d,e,f)),(g,h) = 1,(2,3,(4,5,6)),7"
      node = parse_assign(expr)
      res = PythonUtils.unpack_assign(node.target, node.value)

      assert not res