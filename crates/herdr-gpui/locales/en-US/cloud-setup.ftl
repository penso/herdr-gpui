# What a provider's next cloud device installs and runs, under the switch
# that runs the setup script. $agents lists agent names, which stay as they
# are; $lines counts the lines of the user's own script.

cloud-setup-summary-nothing = Nothing to run.
cloud-setup-summary-runs = { $lines ->
        [one] Runs { $lines } line of your own.
       *[other] Runs { $lines } lines of your own.
    }
cloud-setup-summary-installs = Installs { $agents }.
cloud-setup-summary-installs-runs = { $lines ->
        [one] Installs { $agents }, then runs { $lines } line of your own.
       *[other] Installs { $agents }, then runs { $lines } lines of your own.
    }
