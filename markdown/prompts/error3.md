PID: 577152 (WebKitWebProces)

TID: 577152 (WebKitWebProces)

UID: 1005 (daniel)

GID: 1005 (daniel)

Signal: 11 (SEGV) si_code: SEGV_MAPERR

Timestamp: Fri 2026-09-18 11:57:35 CEST (17s ago)

Command Line: /usr/lib/webkit2gtk-4.1/WebKitWebProcess 4 24

Executable: /usr/lib/webkit2gtk-4.1/WebKitWebProcess

Control Group: /user.slice/user-1005.slice/user@1005.service/app.slice/app-org.kde.konsole-42379.scope/tab(42387).scope

Unit: user@1005.service

User Unit: app-org.kde.konsole-42379.scope

Slice: user-1005.slice

Owner UID: 1005 (daniel)

Boot ID: 55c614295615425992605f857c694df2

Machine ID: 4acf8527aec94b2fa660ab17b964782d

Hostname: archlinux

Storage: /var/lib/systemd/coredump/core.WebKitWebProces.1005.55c614295615425992605f857c694df2.577152.1789725455000000.zst (present)

Size on Disk: 10.8M

Message: Process 577152 (WebKitWebProces) of user 1005 dumped core.

Stack trace of thread 577152:

#0 0x00007fe671bc984e n/a (libnvidia-eglcore.so.615.71.09 + 0x9c984e)

#1 0x00007fe6718fd471 n/a (libnvidia-eglcore.so.615.71.09 + 0x6fd471)

#2 0x00007fe671931fe6 n/a (libnvidia-eglcore.so.615.71.09 + 0x731fe6)

#3 0x00007fe671932058 n/a (libnvidia-eglcore.so.615.71.09 + 0x732058)

#4 0x00007fe6719332ff n/a (libnvidia-eglcore.so.615.71.09 + 0x7332ff)

#5 0x00007fe671903668 n/a (libnvidia-eglcore.so.615.71.09 + 0x703668)

#6 0x00007fe6718f0981 n/a (libnvidia-eglcore.so.615.71.09 + 0x6f0981)

#7 0x00007fe671bb2238 n/a (libnvidia-eglcore.so.615.71.09 + 0x9b2238)

#8 0x00007fe671b8f6cd n/a (libnvidia-eglcore.so.615.71.09 + 0x98f6cd)

#9 0x00007fe6d4d14e9d n/a (libEGL_nvidia.so.0 + 0x40e9d)

#10 0x00007fe6d4d14f72 n/a (libEGL_nvidia.so.0 + 0x40f72)

#11 0x00007fe6d4d16155 n/a (libEGL_nvidia.so.0 + 0x42155)

#12 0x00007fe6d4d1c1e6 n/a (libEGL_nvidia.so.0 + 0x481e6)

#13 0x00007fe6f4064487 n/a (libwebkit2gtk-4.1.so.0 + 0x4464487)

#14 0x00007fe6f1cd83e6 n/a (libwebkit2gtk-4.1.so.0 + 0x20d83e6)

#15 0x00007fe6f1664514 n/a (libwebkit2gtk-4.1.so.0 + 0x1a64514)

#16 0x00007fe6f1c8b966 n/a (libwebkit2gtk-4.1.so.0 + 0x208b966)

#17 0x00007fe6f1cd8309 n/a (libwebkit2gtk-4.1.so.0 + 0x20d8309)

#18 0x00007fe6f1658fbc n/a (libwebkit2gtk-4.1.so.0 + 0x1a58fbc)

#19 0x00007fe6ee8e55ff n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e55ff)

#20 0x00007fe6ee8e587f n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e587f)

#21 0x00007fe6ec056c76 n/a (libglib-2.0.so.0 + 0x61c76)

#22 0x00007fe6ec056f68 g_main_context_dispatch (libglib-2.0.so.0 + 0x61f68)

#23 0x00007fe6ee8e66f7 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66f7)

#24 0x00007fe6ee8e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#25 0x00007fe6f1cd92a8 _ZN6WebKit14WebProcessMainEiPPc (libwebkit2gtk-4.1.so.0 + 0x20d92a8)

#26 0x00007fe6ef827781 n/a (libc.so.6 + 0x27781)

#27 0x00007fe6ef8278b9 __libc_start_main (libc.so.6 + 0x278b9)

#28 0x000055f6e149d635 n/a (WebKitWebProcess + 0x1635)

Stack trace of thread 577155:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911365 __poll (libc.so.6 + 0x111365)

#2 0x00007fe6ee8e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007fe6ee8e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577160:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911365 __poll (libc.so.6 + 0x111365)

#2 0x00007fe6ee8e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007fe6ee8e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577154:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee94b122 n/a (libjavascriptcoregtk-4.1.so.0 + 0x234b122)

#4 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#5 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577156:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911365 __poll (libc.so.6 + 0x111365)

#2 0x00007fe6ee8e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007fe6ee8e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577159:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911365 __poll (libc.so.6 + 0x111365)

#2 0x00007fe6ee8e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007fe6ee8e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577175:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911365 __poll (libc.so.6 + 0x111365)

#2 0x00007fe6ee8e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007fe6ee8e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577183:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911a57 ppoll (libc.so.6 + 0x111a57)

#2 0x00007fe6ec058f59 n/a (libglib-2.0.so.0 + 0x63f59)

#3 0x00007fe6ec059217 g_main_loop_run (libglib-2.0.so.0 + 0x64217)

#4 0x00007fe6f5677ed4 n/a (libgio-2.0.so.0 + 0x11fed4)

#5 0x00007fe6ec08fb14 n/a (libglib-2.0.so.0 + 0x9ab14)

#6 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#7 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588597:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577176:

#0 0x00007fe6ef91e4ed syscall (libc.so.6 + 0x11e4ed)

#1 0x00007fe6ec08615e g_cond_wait (libglib-2.0.so.0 + 0x9115e)

#2 0x00007fe6ec01b52d n/a (libglib-2.0.so.0 + 0x2652d)

#3 0x00007fe6ec090017 n/a (libglib-2.0.so.0 + 0x9b017)

#4 0x00007fe6ec08fb14 n/a (libglib-2.0.so.0 + 0x9ab14)

#5 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577239:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671a3daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007fe671a3e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588601:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577240:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671bdfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007fe671a3e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588599:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588581:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577241:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671bdfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007fe671a3e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577177:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911a57 ppoll (libc.so.6 + 0x111a57)

#2 0x00007fe6ec058f59 n/a (libglib-2.0.so.0 + 0x63f59)

#3 0x00007fe6ec059055 g_main_context_iteration (libglib-2.0.so.0 + 0x64055)

#4 0x00007fe6ec0590a2 n/a (libglib-2.0.so.0 + 0x640a2)

#5 0x00007fe6ec08fb14 n/a (libglib-2.0.so.0 + 0x9ab14)

#6 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#7 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588582:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577243:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671bdfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007fe671a3e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588590:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577242:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671a3daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007fe671a3e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577244:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671bdfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007fe671a3e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588598:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588600:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577245:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671bdfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007fe671a3e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 577246:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe671b7b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007fe671bdfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007fe671a3e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007fe671b7edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588651:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef911365 __poll (libc.so.6 + 0x111365)

#2 0x00007fe6ee8e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007fe6ee8e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588580:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588583:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588584:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588585:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588586:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588591:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588595:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588596:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 588623:

#0 0x00007fe6ef8a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007fe6ef894cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007fe6ef897762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007fe6ee8ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007fe6ee8061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007fe6ee7d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007fe6ee8ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007fe6ef8980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007fe6ef92080c n/a (libc.so.6 + 0x12080c)

ELF object binary architecture: AMD x86-64