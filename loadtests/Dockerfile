FROM loadimpact/k6:latest
# Copy your test script into the image
COPY complete_test.js /tests/complete_test.js
# Set the entrypoint to run your script by default
ENTRYPOINT ["k6", "run", "/tests/complete_test.js"]
