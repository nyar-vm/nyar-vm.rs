-- mid-subset: tables, multi-assign, loops, functions
local t = {1, 2, name = "victory"}
local a, b = 10, 20
local sum = 0
for i = 1, #t do
  sum = sum + t[i]
end
local n = 0
repeat
  n = n + 1
until n >= 2
function greet(who)
  return who .. ":" .. (sum + a + b + n)
end
if sum == 3 and n == 2 then
  print(greet(t.name))
end
