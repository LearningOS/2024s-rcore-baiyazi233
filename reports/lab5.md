调用take_current_task()方法获得当前进程会使Processor的current字段变为None，所以出现了许多因为None导致.unwrap()的panic。我选择加一个set_current的方法，给这拿出来的current重新设置为current。
