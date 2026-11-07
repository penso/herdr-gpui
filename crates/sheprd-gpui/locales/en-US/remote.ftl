# The other machines a paired phone reaches, under its name in Settings >
# Remote control. $names lists them, separated by commas; the app joins
# the states it has with " · ".
remote-hosts =
    { $state ->
        [paired] Also on { $names }
        [pairing] pairing { $names }
       *[failed] failed on { $names }
    }
