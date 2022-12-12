class ArrayList<T> {
    _items: [T]
}
imply ArrayList<T> {
    micro push(mut self, value: T): unit {
        return
    }
    micro length(self): usize { return 0 }
}
class BinaryHeap<T> {
    data: List<T>
}
imply BinaryHeap<T> {
    micro push(mut self, value: T): unit {
        self.data.push(value)
    }
}
