#!/usr/bin/env bash
name=victory
greet() {
  printf "%s" "$1"
}
x=0
while [ $x -lt 1 ]; do
  x=1
done
if [ $x -eq 1 ]; then
  for item in "$name"; do
    greet $item
  done
fi
