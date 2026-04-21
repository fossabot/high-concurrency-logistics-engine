FROM loadimpact/k6:latest
# Copy your test script into the image
COPY driver.js /tests/driver.js
# Set the entrypoint to run your script by default
ENTRYPOINT ["k6", "run", "/tests/driver.js"]
