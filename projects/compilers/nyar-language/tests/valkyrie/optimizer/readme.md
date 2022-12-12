# compiler tests optimizer

这里验证优化是否保持语义等价，避免优化层偷偷承担 lowering 修补工作。见证消除（witness elimination）针对封闭类的 trait 动态派发。

