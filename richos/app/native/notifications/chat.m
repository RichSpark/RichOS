#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <UserNotifications/UserNotifications.h>

extern void richos_open_chat_notification(const char *thread);
extern void richos_chat_clock_changed(void);
extern void richos_chat_notification_observation(unsigned int event);

@interface RichChatNotificationDelegate : NSObject <UNUserNotificationCenterDelegate>
@end

@implementation RichChatNotificationDelegate
- (void)userNotificationCenter:(UNUserNotificationCenter *)center
 didReceiveNotificationResponse:(UNNotificationResponse *)response
          withCompletionHandler:(void (^)(void))done {
    NSString *thread = response.notification.request.content.userInfo[@"chatThread"];
    if (thread.length && [response.actionIdentifier isEqualToString:UNNotificationDefaultActionIdentifier]) {
        richos_open_chat_notification(thread.UTF8String);
    }
    done();
}
@end

static UNUserNotificationCenter *rich_center(void) {
    if (!NSBundle.mainBundle.bundleIdentifier) return nil;
    UNUserNotificationCenter *center = UNUserNotificationCenter.currentNotificationCenter;
    static RichChatNotificationDelegate *delegate;
    if (!delegate) {
        delegate = [RichChatNotificationDelegate new];
        center.delegate = delegate;
    }
    return center;
}

void richos_chat_notifications_install(void) {
    (void)rich_center();
    [NSWorkspace.sharedWorkspace.notificationCenter
        addObserverForName:NSWorkspaceDidWakeNotification object:nil queue:nil
        usingBlock:^(NSNotification *note) { (void)note; richos_chat_clock_changed(); }];
    [NSNotificationCenter.defaultCenter
        addObserverForName:NSSystemClockDidChangeNotification object:nil queue:nil
        usingBlock:^(NSNotification *note) { (void)note; richos_chat_clock_changed(); }];
}

static void rich_sound(NSString *file) {
    if (!file.length) return;
    NSURL *url = [NSBundle.mainBundle URLForResource:file withExtension:nil subdirectory:@"ChatSounds"];
    if (url && getenv("RICHOS_CHAT_SOUNDS_TRACE_ONLY")) {
        richos_chat_notification_observation(0);
        fprintf(stderr, "[richos] chat-list sound=%s outcome=trace-only\n", file.UTF8String);
        return;
    }
    static NSMutableDictionary<NSString *, AVAudioPlayer *> *players;
    NSError *error = nil;
    AVAudioPlayer *sound = url ? [[AVAudioPlayer alloc] initWithContentsOfURL:url error:&error] : nil;
    if (!players) players = [NSMutableDictionary new];
    if (sound) players[file] = sound;
    BOOL playing = [sound play];
    richos_chat_notification_observation(playing ? 1 : url ? 2 : 3);
    fprintf(stderr, "[richos] chat-list sound=%s outcome=%s\n", file.UTF8String,
        playing ? "started" : url ? "failed" : "unavailable");
}

void richos_chat_answer_sound(const char *file) {
    rich_sound([NSString stringWithUTF8String:file]);
}

void richos_chat_notification(const char *thread, const char *identifier, const char *title,
                              const char *body, const char *file, bool show, bool playWhenActive) {
    NSString *sound = [NSString stringWithUTF8String:file];
    if (NSApp.isActive) {
        if (playWhenActive) rich_sound(sound);
        return;
    }
    if (!show) return;
    UNUserNotificationCenter *center = rich_center();
    if (!center) {
        richos_chat_notification_observation(4);
        fprintf(stderr, "[richos] chat-list notification=unavailable\n");
        return;
    }
    UNMutableNotificationContent *content = [UNMutableNotificationContent new];
    content.title = [NSString stringWithUTF8String:title];
    content.body = [NSString stringWithUTF8String:body];
    content.userInfo = @{@"chatThread": [NSString stringWithUTF8String:thread]};
    UNNotificationRequest *request = [UNNotificationRequest
        requestWithIdentifier:[NSString stringWithUTF8String:identifier] content:content trigger:nil];
    [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert completionHandler:^(BOOL granted, NSError *error) {
        if (!granted) {
            richos_chat_notification_observation(error ? 5 : 6);
            fprintf(stderr, "[richos] chat-list notification=%s\n", error ? "permission-error" : "permission-denied");
            return;
        }
        dispatch_async(dispatch_get_main_queue(), ^{
            if (NSApp.isActive) return;
            [center addNotificationRequest:request withCompletionHandler:^(NSError *error) {
                richos_chat_notification_observation(error ? 7 : 8);
                fprintf(stderr, "[richos] chat-list notification=%s\n", error ? "delivery-error" : "submitted");
                if (!error && sound.length) dispatch_async(dispatch_get_main_queue(), ^{ if (!NSApp.isActive) rich_sound(sound); });
            }];
        });
    }];
}
