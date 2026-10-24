set name victory
proc greet {who} {
  return $who
}
set x 0
while {$x < 1} {
  incr x
}
if {$x == 1} {
  for {set i 0} {$i < 1} {incr i} {
    puts [greet ${name}]
  }
}
