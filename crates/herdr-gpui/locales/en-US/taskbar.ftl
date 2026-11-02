# Windows' taskbar button. A screen reader reads this for the badge that
# counts the agents waiting for the user.
taskbar-attention =
    { $count ->
        [one] { $count } agent needs attention
       *[other] { $count } agents need attention
    }
