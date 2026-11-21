// Present the whole authored timeline in the app's own window.
#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <QuartzCore/QuartzCore.h>
#import <stdatomic.h>

enum { Width = 2800, Height = 1704, FPS = 60, FadeMs = 180, PrerollFrames = 4 };
static const int VideoFailed = -1;
static const int VideoSurfaceLost = -2;
static const int VideoLateFrames = -3;
static const int VideoPreroll = 0;
static const int VideoPlaying = 1;
static const int VideoEnded = 2;
typedef void (*RichSplashPhase)(int, double);
typedef void (*RichSplashMetrics)(uint64_t, uint64_t, uint64_t);
typedef void (*RichSplashAudio)(int);
typedef enum {
    SoundDisabled,
    SoundScheduled,
    SoundPrepareFailed,
    SoundDeadlineMissed,
    SoundScheduleRefused,
    SoundFinished,
    SoundInterrupted,
    SoundCompletionTimedOut,
} SoundOutcome;

// Sound is owned by the launch cue even after the native window is removed.
@interface RichSplashCue : NSObject <AVAudioPlayerDelegate>
@property(nonatomic, strong) AVAudioPlayer *audio;
@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic) RichSplashAudio report;
@end

@implementation RichSplashCue
- (void)finish:(SoundOutcome)outcome {
    if (!self.audio) return;
    self.audio.delegate = nil;
    [self.audio stop];
    self.audio = nil;
    if (self.report) self.report(outcome);
}

- (void)audioPlayerDidFinishPlaying:(AVAudioPlayer *)audio successfully:(BOOL)success {
    dispatch_async(self.queue, ^{ [self finish:success ? SoundFinished : SoundInterrupted]; });
}

- (void)audioPlayerDecodeErrorDidOccur:(AVAudioPlayer *)audio error:(NSError *)error {
    dispatch_async(self.queue, ^{ [self finish:SoundInterrupted]; });
}
@end

static void scheduleCue(NSString *path, double length, CMTime anchor, RichSplashAudio report) {
    dispatch_queue_t queue = dispatch_queue_create("com.richos.splash.sound",
        dispatch_queue_attr_make_with_qos_class(DISPATCH_QUEUE_SERIAL, QOS_CLASS_USER_INITIATED, 0));
    dispatch_async(queue, ^{
        if (!path) {
            if (report) report(SoundDisabled);
            return;
        }
        AVAudioPlayer *audio = [[AVAudioPlayer alloc] initWithContentsOfURL:[NSURL fileURLWithPath:path] error:nil];
        if (![audio prepareToPlay]) {
            if (report) report(SoundPrepareFailed);
            return;
        }

        // Start the cue 130 ms before the bar lands, 800 ms from the end.
        NSTimeInterval deviceTime = audio.deviceCurrentTime;
        double elapsed = CMTimeGetSeconds(CMTimeSubtract(CMClockGetTime(CMClockGetHostTimeClock()), anchor));
        double lead = length - 0.930 - elapsed;
        if (lead <= 0) {
            if (report) report(SoundDeadlineMissed);
            return;
        }
        RichSplashCue *cue = [RichSplashCue new];
        cue.audio = audio;
        cue.queue = queue;
        cue.report = report;
        audio.delegate = cue;
        if (![audio playAtTime:deviceTime + lead]) {
            if (report) report(SoundScheduleRefused);
            return;
        }

        // Keep the cue alive until the delegate completes it. The guard also
        // releases the device if a detached output never sends a callback.
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)((lead + audio.duration + 2.0) * NSEC_PER_SEC)),
                       queue, ^{ [cue finish:SoundCompletionTimedOut]; });
        if (report) report(SoundScheduled);
    });
}

@interface RichSplashCover : NSView
@property(nonatomic, copy) dispatch_block_t lostWindow;
@end

@implementation RichSplashCover
- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    if (!self.window && self.lostWindow) self.lostWindow();
}

- (NSView *)hitTest:(NSPoint)point {
    return [super hitTest:point] ? self : nil;
}
- (void)mouseDown:(NSEvent *)event {}
- (void)mouseUp:(NSEvent *)event {}
- (void)rightMouseDown:(NSEvent *)event {}
- (void)rightMouseUp:(NSEvent *)event {}
- (void)otherMouseDown:(NSEvent *)event {}
- (void)otherMouseUp:(NSEvent *)event {}
- (void)scrollWheel:(NSEvent *)event {}
@end

// The cover is its own compositor surface, owned by the app's one main window.
@interface RichSplashPanel : NSPanel
@end

@implementation RichSplashPanel
- (BOOL)canBecomeKeyWindow { return NO; }
- (BOOL)canBecomeMainWindow { return NO; }
@end

static NSRect contentOnScreen(NSView *view) {
    return [view.window convertRectToScreen:[view convertRect:view.bounds toView:nil]];
}

@interface RichSplashPlayer : NSObject {
    atomic_int _status;
    AVAssetReader *_reader;
    AVAssetReaderTrackOutput *_frames;
    CMSampleBufferRef _preroll[PrerollFrames];
    int _frameIndex;                 // preparation, then media queue
    int _frameCount;
    atomic_int _queuedFrames;
    id _endObserver;
    id _readyObserver;
    id _failedObserver;
    id _sizeObserver;
    BOOL _watchingStatus;
    BOOL _removing;                 // main thread
    BOOL _visible;                  // timeline queue
    BOOL _ready;                    // timeline queue
}
@property(nonatomic, strong) RichSplashCover *cover;
@property(nonatomic, strong) RichSplashPanel *panel;
@property(nonatomic, strong) AVSampleBufferDisplayLayer *video;
@property(nonatomic, strong) id<AVQueuedSampleBufferRendering> renderer;
@property(nonatomic, strong) dispatch_queue_t timeline;
@property(nonatomic, strong) dispatch_queue_t media;
@property(nonatomic, strong) AVSampleBufferRenderSynchronizer *clock;
@property(nonatomic, copy) NSString *soundPath;
@property(nonatomic) double anchorMs;
@property(nonatomic) RichSplashPhase phase;
@property(nonatomic) RichSplashMetrics metrics;
@property(nonatomic) RichSplashAudio audioOutcome;
- (instancetype)initWithPack:(NSString *)path frames:(int)count;
- (BOOL)prepare;
- (void)attach:(NSView *)view;
- (void)visible;
- (int)status;
- (void)removeAnimated:(BOOL)animated completion:(dispatch_block_t)completion;
@end

@implementation RichSplashPlayer

- (instancetype)initWithPack:(NSString *)path frames:(int)count {
    self = [super init];
    if (!self || (count != 3 * FPS && count != 5 * FPS)) return nil;
    AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:path] options:nil];
    AVAssetTrack *track = [asset tracksWithMediaType:AVMediaTypeVideo].firstObject;
    if (!track || CMTimeCompare(track.timeRange.duration, CMTimeMake(count, FPS)) != 0) return nil;
    CMVideoFormatDescriptionRef format = (__bridge CMVideoFormatDescriptionRef)track.formatDescriptions.firstObject;
    if (!format || CMFormatDescriptionGetMediaSubType(format) != kCMVideoCodecType_H264) return nil;
    CMVideoDimensions dimensions = CMVideoFormatDescriptionGetDimensions(format);
    if (dimensions.width != Width || dimensions.height != Height) return nil;

    _reader = [[AVAssetReader alloc] initWithAsset:asset error:nil];
    // The packs have no reordered frames. Send their H.264 samples directly to
    // the display renderer so startup I/O cannot stall a separate NV12 decode.
    _frames = [[AVAssetReaderTrackOutput alloc] initWithTrack:track outputSettings:nil];
    _frames.alwaysCopiesSampleData = NO;
    if (![_reader canAddOutput:_frames]) return nil;
    [_reader addOutput:_frames];
    if (![_reader startReading]) return nil;
    _frameCount = count;
    atomic_init(&_status, VideoPreroll);
    atomic_init(&_queuedFrames, 0);
    _media = dispatch_queue_create("com.richos.splash.media",
        dispatch_queue_attr_make_with_qos_class(DISPATCH_QUEUE_SERIAL, QOS_CLASS_USER_INITIATED, 0));
    return self;
}

- (void)dealloc {
    for (int i = 0; i < PrerollFrames; i++) {
        if (_preroll[i]) CFRelease(_preroll[i]);
    }
    [_reader cancelReading];
}

// Keep each sample's authored timestamp all the way to the display renderer.
- (CMSampleBufferRef)nextSample CF_RETURNS_RETAINED {
    CMSampleBufferRef sample;
    while ((sample = [_frames copyNextSampleBuffer])) {
        if (CMSampleBufferGetNumSamples(sample) > 0) break;
        // AVAssetReader also emits empty timeline markers. The movie has no frame there.
        CFRelease(sample);
    }
    if (!sample) {
        fprintf(stderr, "[richos] native splash: decoder stopped at frame %d of %d (reader status %ld)\n",
                _frameIndex, _frameCount, (long)_reader.status);
        return NULL;
    }
    CMTime pts = CMSampleBufferGetPresentationTimeStamp(sample);
    if (CMTimeCompare(pts, CMTimeMake(_frameIndex, FPS)) != 0) {
        fprintf(stderr, "[richos] native splash: unexpected sample time %.3f s at frame %d of %d\n",
                CMTimeGetSeconds(pts), _frameIndex, _frameCount);
        CFRelease(sample);
        return NULL;
    }
    _frameIndex++;
    return sample;
}

- (int)status { return atomic_load(&_status); }

- (void)unwatch {
    if (_sizeObserver) {
        [NSNotificationCenter.defaultCenter removeObserver:_sizeObserver];
        _sizeObserver = nil;
    }
    if (_readyObserver) {
        [NSNotificationCenter.defaultCenter removeObserver:_readyObserver];
        _readyObserver = nil;
    }
    if (_watchingStatus) {
        _watchingStatus = NO;
        [self.video removeObserver:self forKeyPath:@"status"];
    }
    if (_failedObserver) {
        [NSNotificationCenter.defaultCenter removeObserver:_failedObserver];
        _failedObserver = nil;
    }
}

- (void)stopFeeding {
    dispatch_async(self.media, ^{ [self.renderer stopRequestingMediaData]; });
}

// Called on the timeline queue.
- (void)fail:(int)reason {
    int status = self.status;
    if (status != VideoPreroll && status != VideoPlaying) return;
    atomic_store(&_status, VideoFailed);
    [self stopFeeding];
    [self.clock setRate:0 time:self.clock.currentTime];
    self.phase(reason, self.anchorMs);
}

- (void)feed {
    __weak RichSplashPlayer *weak = self;
    [self.renderer requestMediaDataWhenReadyOnQueue:self.media usingBlock:^{
        RichSplashPlayer *player = weak;
        if (!player) return;
        while (player.renderer.readyForMoreMediaData && player->_frameIndex < player->_frameCount && player.status != VideoFailed) {
            @autoreleasepool {
                CMSampleBufferRef sample = [player nextSample];
                if (!sample) {
                    dispatch_async(player.timeline, ^{ [player fail:VideoFailed]; });
                    [player.renderer stopRequestingMediaData];
                    return;
                }
                if (player.status != VideoFailed) {
                    [player.renderer enqueueSampleBuffer:sample];
                    atomic_fetch_add(&player->_queuedFrames, 1);
                }
                CFRelease(sample);
            }
        }
        if (player->_frameIndex == player->_frameCount || player.status == VideoFailed) [player.renderer stopRequestingMediaData];
    }];
}

// The window must reach AppKit before the clock consumes video frames. Audio
// takes the same clock anchor, with its own queue so the video feed cannot delay it.
- (void)start {
    if (!_visible || !_ready) return;
    int expected = VideoPreroll;
    if (!atomic_compare_exchange_strong(&_status, &expected, VideoPlaying)) return;
    CMTime time = CMClockGetTime(CMClockGetHostTimeClock());
    self.anchorMs = NSDate.date.timeIntervalSince1970 * 1000;
    __weak RichSplashPlayer *weak = self;
    _endObserver = [self.clock addBoundaryTimeObserverForTimes:@[[NSValue valueWithCMTime:CMTimeMake(_frameCount, FPS)]]
                                                    queue:self.timeline usingBlock:^{
        RichSplashPlayer *player = weak;
        if (player && player.status == VideoPlaying) {
            int queuedFrames = atomic_load(&player->_queuedFrames);
            if (queuedFrames != player->_frameCount) {
                fprintf(stderr, "[richos] native splash: renderer accepted %d of %d frames at the endpoint\n",
                        queuedFrames, player->_frameCount);
                [player fail:VideoLateFrames];
                return;
            }
            atomic_store(&player->_status, VideoEnded);
            player.phase(VideoEnded, player.anchorMs);
            if (@available(macOS 14.4, *)) {
                RichSplashMetrics report = player.metrics;
                [player.video.sampleBufferRenderer loadVideoPerformanceMetricsWithCompletionHandler:
                    ^(AVVideoPerformanceMetrics *metrics) {
                    if (metrics && report) {
                        report((uint64_t)metrics.totalNumberOfFrames, (uint64_t)metrics.numberOfDroppedFrames,
                               (uint64_t)metrics.numberOfCorruptedFrames);
                    }
                }];
            }
        }
    }];
    [self.clock setRate:1 time:kCMTimeZero atHostTime:time];
    scheduleCue(self.soundPath, (double)_frameCount / FPS, time, self.audioOutcome);
    self.phase(VideoPlaying, self.anchorMs);

    // If the page never painted, the final frame may stay briefly while it loads.
    // A failed WebView cannot leave the native cover permanently in the window.
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)((_frameCount / FPS + 3) * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
        RichSplashPlayer *player = weak;
        if (player && !player->_removing) [player removeAnimated:NO completion:^{}];
    });
}

- (void)rendererReady {
    __weak RichSplashPlayer *weak = self;
    dispatch_async(self.timeline, ^{
        RichSplashPlayer *player = weak;
        if (!player) return;
        player->_ready = YES;
        [player start];
    });
}

- (void)observeValueForKeyPath:(NSString *)keyPath ofObject:(id)object change:(NSDictionary *)change context:(void *)context {
    if ([change[NSKeyValueChangeNewKey] integerValue] == AVQueuedSampleBufferRenderingStatusRendering) {
        [self rendererReady];
    }
}

- (void)visible {
    __weak RichSplashPlayer *weak = self;
    dispatch_async(self.timeline, ^{
        RichSplashPlayer *player = weak;
        if (!player) return;
        player->_visible = YES;
        [player start];
    });
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), self.timeline, ^{
        RichSplashPlayer *player = weak;
        if (player && player.status == VideoPreroll) {
            fputs("[richos] native splash: display layer did not become ready\n", stderr);
            [player fail:VideoFailed];
        }
    });
}

- (BOOL)prepare {
    for (int i = 0; i < PrerollFrames; i++) {
        _preroll[i] = [self nextSample];
        if (!_preroll[i]) return NO;
    }
    return YES;
}

- (void)attach:(NSView *)view {
    NSWindow *parent = view.window;
    self.panel = [[RichSplashPanel alloc] initWithContentRect:contentOnScreen(view)
        styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
        backing:NSBackingStoreBuffered defer:NO];
    self.panel.releasedWhenClosed = NO;
    self.panel.opaque = YES;
    self.panel.hasShadow = NO;
    self.panel.excludedFromWindowsMenu = YES;
    self.panel.collectionBehavior = NSWindowCollectionBehaviorTransient | NSWindowCollectionBehaviorIgnoresCycle;
    self.panel.backgroundColor = parent.backgroundColor;
    self.cover = [[RichSplashCover alloc] initWithFrame:self.panel.contentView.bounds];
    self.cover.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    CALayer *surface = [CALayer layer];
    surface.frame = self.cover.bounds;
    surface.zPosition = 1;
    surface.masksToBounds = YES;
    self.cover.layer = surface;
    self.cover.wantsLayer = YES;
    self.video = [AVSampleBufferDisplayLayer layer];
    self.video.frame = self.cover.bounds;
    self.video.autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable;
    self.video.videoGravity = AVLayerVideoGravityResizeAspectFill;
    [surface addSublayer:self.video];
    self.timeline = dispatch_queue_create("com.richos.splash.timeline",
        dispatch_queue_attr_make_with_qos_class(DISPATCH_QUEUE_SERIAL, QOS_CLASS_USER_INTERACTIVE, 0));
    if (@available(macOS 14.0, *)) self.renderer = self.video.sampleBufferRenderer;
    else self.renderer = self.video;
    self.clock = [AVSampleBufferRenderSynchronizer new];
    self.clock.delaysRateChangeUntilHasSufficientMediaData = NO;
    [self.clock addRenderer:self.renderer];
    [self.clock setRate:0 time:kCMTimeZero];

    __weak RichSplashPlayer *weak = self;
    NSNotificationName failed = AVSampleBufferDisplayLayerFailedToDecodeNotification;
    if (@available(macOS 14.0, *)) failed = AVSampleBufferVideoRendererDidFailToDecodeNotification;
    _failedObserver = [NSNotificationCenter.defaultCenter addObserverForName:failed
        object:self.renderer queue:nil usingBlock:^(NSNotification *note) {
            (void)note;
            RichSplashPlayer *player = weak;
            if (player) dispatch_async(player.timeline, ^{ [player fail:VideoFailed]; });
        }];
    for (int i = 0; i < PrerollFrames; i++) {
        [self.renderer enqueueSampleBuffer:_preroll[i]];
        atomic_fetch_add(&_queuedFrames, 1);
        CFRelease(_preroll[i]);
        _preroll[i] = NULL;
    }
    if (@available(macOS 14.4, *)) {
        _readyObserver = [NSNotificationCenter.defaultCenter addObserverForName:AVSampleBufferDisplayLayerReadyForDisplayDidChangeNotification
            object:self.video queue:nil usingBlock:^(NSNotification *notification) {
                AVSampleBufferDisplayLayer *video = notification.object;
                if (video.readyForDisplay) [weak rendererReady];
            }];
        if (self.video.readyForDisplay) [self rendererReady];
    } else {
        _watchingStatus = YES;
        [self.video addObserver:self forKeyPath:@"status" options:NSKeyValueObservingOptionInitial | NSKeyValueObservingOptionNew context:NULL];
    }
    self.panel.contentView = self.cover;
    [parent addChildWindow:self.panel ordered:NSWindowAbove];
    __weak NSView *hostView = view;
    _sizeObserver = [NSNotificationCenter.defaultCenter addObserverForName:NSWindowDidResizeNotification
        object:parent queue:nil usingBlock:^(NSNotification *note) {
            (void)note;
            NSView *host = hostView;
            if (host) [weak.panel setFrame:contentOnScreen(host) display:NO];
        }];
    self.cover.lostWindow = ^{
        RichSplashPlayer *player = weak;
        if (player) dispatch_async(player.timeline, ^{ [player fail:VideoSurfaceLost]; });
    };
    // Read ahead while the first frame becomes ready. The renderer controls its
    // own backpressure and its reader never occupies the clock's queue.
    [self feed];
}

- (void)closeSurface {
    [self.panel.parentWindow removeChildWindow:self.panel];
    [self.panel close];
    self.panel = nil;
    self.cover = nil;
}

- (void)removeAnimated:(BOOL)animated completion:(dispatch_block_t)completion {
    if (_removing) {
        completion();
        return;
    }
    _removing = YES;
    int expected = VideoPreroll;
    atomic_compare_exchange_strong(&_status, &expected, VideoFailed);
    self.cover.lostWindow = nil;
    [self unwatch];
    dispatch_async(self.timeline, ^{
        if (self->_endObserver) {
            [self.clock removeTimeObserver:self->_endObserver];
            self->_endObserver = nil;
        }
        [self stopFeeding];
        [self.clock setRate:0 time:self.clock.currentTime];
    });
    if (!animated) {
        [self closeSurface];
        completion();
        return;
    }
    [NSAnimationContext runAnimationGroup:^(NSAnimationContext *context) {
        context.duration = FadeMs / 1000.0;
        context.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionLinear];
        self.panel.animator.alphaValue = 0;
    } completionHandler:^{
        [self closeSurface];
        fputs("[richos] native splash: fade complete\n", stderr);
        completion();
    }];
}

@end

bool richos_splash_reduce_motion(void) {
    return NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion;
}

// Read the first video samples while the main thread builds the window.
void *richos_splash_prepare(const char *file, int frames, const char *sound,
                            RichSplashPhase phase, RichSplashMetrics metrics, RichSplashAudio audioOutcome) {
    @autoreleasepool {
        NSString *path = [NSFileManager.defaultManager stringWithFileSystemRepresentation:file length:strlen(file)];
        RichSplashPlayer *player = [[RichSplashPlayer alloc] initWithPack:path frames:frames];
        player.phase = phase;
        player.metrics = metrics;
        player.audioOutcome = audioOutcome;
        if (sound) player.soundPath = [NSFileManager.defaultManager stringWithFileSystemRepresentation:sound length:strlen(sound)];
        if (![player prepare]) return NULL;
        return (__bridge_retained void *)player;
    }
}

void richos_splash_discard_prepared(void *handle) {
    CFRelease((CFTypeRef)handle);
}

// Attach the prepared layer to AppKit on the main thread.
bool richos_splash_attach(void *window, void *handle) {
    NSView *view = [(__bridge NSWindow *)window contentView];
    if (!view || !view.window) return false;
    [(__bridge RichSplashPlayer *)handle attach:view];
    return true;
}

void richos_splash_window_visible(void *handle) {
    // An explicit window flush can put its background on screen while AppKit
    // is still busy. Let the launch hook return before starting the movie clock.
    __weak RichSplashPlayer *player = (__bridge RichSplashPlayer *)handle;
    dispatch_async(dispatch_get_main_queue(), ^{ [player visible]; });
}

void richos_splash_remove(void *handle, bool animated, void (*done)(void *), void *data) {
    RichSplashPlayer *player = (__bridge_transfer RichSplashPlayer *)handle;
    [player removeAnimated:animated completion:^{ if (done) done(data); }];
}
