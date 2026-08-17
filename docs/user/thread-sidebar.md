# Organizing threads

Pin a thread from its context menu to keep it in the pinned section above your active work.
Pinned threads are shown independently of their project, including when you connect to more than
one environment.

On web and desktop, drag a pinned thread to change its position. On mobile, open the thread's menu
and choose **Move up** or **Move down**. The order is stored by the server and appears on your
other connected devices.

If reordering is unavailable for one environment, update the T3 Code server running in that
environment. Older servers can still pin and unpin threads, but do not understand synced ordering;
their pinned threads keep the default newest-first order below the ones you have arranged.

## Add a project by dropping a folder

In the desktop app, drag a folder from your file manager onto the sidebar. T3 Code opens **Add
project** with that folder already filled in, so you confirm it with **Add** or Enter. An empty
sidebar says so next to its **Add project** button.

The folder is added to this device's environment, the one the desktop app runs itself. To add a
folder that lives on another machine, use **Add project** and browse that environment instead.

Browsers do not tell an app where a dropped folder lives on disk, so the sidebar in a browser tab
is not a drop target. Use **Add project** there.

## Environment artwork

Dev and Nightly environments can identify themselves with artwork at the top of the sidebar and in
the send button. Choose **Artwork**, **Version pill**, or **None** in Settings under environment
identification. Artwork is recolored to match each built-in theme. Custom themes use the **Version
pill** fallback because their colors are not controlled by T3 Code.

To generate a fresh title from the conversation, open a thread's context menu and choose
**Regenerate title**. While T3 Code is generating it, the action reads **Regenerating…** and cannot
be selected again. The option is hidden when the connected environment needs a server update.
