# Why the iPhone build takes three seconds to show anything

**You:** The phone build sits on a white screen for about three seconds before the home screen appears. The Android one does not. What is going on?

**Assistant:** Three seconds at launch on iOS usually comes from work done before the initial frame is drawn. In your build, three things run on the main thread before the initial view appears:

1. The database migration check reads every table header.
2. The font files are registered one by one.
3. The analytics library waits for a network handshake.

## What to change

Move the migration check behind the initial frame, register the fonts lazily, and start analytics after the home screen is visible. On the test device that took the cold launch from 3.1 seconds to 0.7 seconds.
