# Runtime Plugin Interface



Pushing things out of the CLI that don't need to be in the core.



### Global Policy



Policies applying to all plugins.



#### Loading

* Plugins are native libraries loaded at runtime through a C ABI.
* Libraries are loaded from a directory named `plugins` next to the executable.
* On Linux, the application is assumed to be installed in a user directory, to keep the same layout.
* Libraries are loaded in order of file name.
* An incompatible library is rejected and flagged.
* A library may provide one or more plugins, of any mix of types.
* Plugin names are unique. If two plugins share a name, the first one loaded is used. Later ones are rejected and flagged, and the rest of their library still loads.
* A plugin missing a function that its type requires is rejected and flagged. It does not claim its name.
* Loading problems never stop the application. A section or source that needs a plugin that did not load is flagged.
* A section or source that names a plugin of the wrong type is flagged in the same way, such as a data source placed in a section, or a button given a `source`.
* Buttons come from plugins, with no built-in ones. Running a program is the `run` verb of the `winbuttons` plugin.
* Images and text labels are handled by the application for now.
* Plugins are trusted code running in the application's process. There is no sandbox.



#### Execution

* The host creates one OS thread per plugin instance and makes all calls to that instance from it.
* The main loop (touch, navigation, frame output) never waits on a plugin. It composites the last buffer each instance published.
* Calls may block. A plugin may start its own threads or async runtime, and must stop them at shutdown.
* A slow instance keeps its last picture. Everything else keeps running.
* At exit, the host calls shutdown() on every instance and waits up to 2 seconds for them. It then closes anyway, so an instance stuck in a call may never receive shutdown().
* Panics and exceptions must not cross the plugin boundary.



#### Failures

* An error from a plugin never stops the application.
* To discard an instance, the host calls shutdown() and shows an error placeholder in its area.
* An instance is discarded if it fails during startup, panics, returns a picture of the wrong size, or fails to render three times in a row.
* A data source is also discarded if it returns bad data, such as series that differ from those get\_series() reported, or fails to poll three times in a row.
* After any other error the instance remains usable. The error shows over its area for 3 seconds. A data source has no area, so its errors show over the visuals that use it.
* After a caught panic, the only further call is shutdown().
* The host flags failures to the user, on the display and in its log, naming the screen and section (or the source), the plugin and the plugin's message.
* When a data source is discarded, every visual that uses it is shut down and flagged, even if its other sources still work.
* A visual that names a series its source does not supply is flagged and not started.
* This covers errors a plugin reports. A crash inside a plugin still takes down the application.



#### Instances

* A plugin may be instantiated any number of times, including twice on one screen.
* State belongs to the instance. Anything shared between instances must be thread-safe.



#### Configuration

* Plugin config must be representable in YAML.
* Each instance's plugin settings go in a `config` block in the app-level YAML. The host passes it to the plugin without interpreting it, along with the config file's folder for relative paths.
* Keys outside the `config` block, such as `plugin`, `width` and `source`, belong to the host.
* The plugin validates its config, rejects unknown keys and returns a readable error. The host adds the screen and section name (or the source name) and treats it as a failure (see Failures).
* Defaults come from the plugin. The host never rewrites the user's file.
* Secrets are referenced (environment variable or file path), not written inline.



#### Text Rendering

* The host offers a text rendering service, so fonts and color emoji behave consistently. It can be called from any plugin thread.
* Using it is optional. Within its own area, a plugin may render text, or anything else, however it likes.



#### Screen Space

* A section is 10-640 px wide and 48 px tall. A full screen is a 640 px section, not a separate mode.
* A section holds one item, or a vertical stack of passive items (data visuals, labels and images).
* Stacked items are listed top to bottom. They share the section's width, each has its own `height`, and the heights total no more than 48 px.
* A stack holds at most 4 items, each at least 10 px tall.
* A stacked label with no `size` has text three quarters of its height, up to 30 px.
* Buttons and scroll bars take the full section height.
* An instance draws only within its own area.
* Data sources take no screen space.



#### Visibility

* Data sources run whether or not they are on screen, so history stays continuous.
* Visuals need a current buffer when visible or on an adjacent screen, so swipes never reveal a blank.



### Touch Handling

The Nexus supports touch and horizontal swipes. Y coordinates are not available.



* Flicks are owned by the application
* Taps and slow drags on interactive sections are routed to plugins



#### Ownership

* Every gesture has one owner, either the application or a single plugin instance.
* Ownership is decided before anything reaches a plugin, and does not change until the finger lifts.
* There is no cancel. A plugin never has to undo a touch event.



#### Passive Sections

Data visuals, labels and images.

* The application owns the gesture immediately, and drags move the screen.



#### Interactive Sections

Buttons and scroll bars.

* The host holds events until the finger lifts or 100 ms pass, whichever comes first.
* If the finger has moved more than 16 px and is at flick speed (initially 0.4 px/ms), the application owns the gesture and changes screens.
* Otherwise the instance owns it. It receives the held events, then live ones.
* The instance keeps the gesture even if the finger slows, speeds up or leaves the section.
* A quick tap reaches the instance on lift.
* While an instance is busy, the host keeps only the latest move. down, up and lost are always delivered, in order.
* If touch reports stop for 300 ms without a lift, the instance receives a lost phase.
* Until an instance has drawn its first picture, and after it has been discarded, its section behaves as a passive section.

The 100 ms window and the flick speed are starting points, to be tuned after the initial plugins are built.



### Types



#### Buttons



Rather than having our internally generated buttons, we let plugins offer them.



Buttons offer verbs, which perform an action. This can include running a program, calling a specific API, etc.



Buttons can offer styles. Buttons cannot stack due to limitations on touch detection.



* A button shows as pressed once it receives the touch. On a quick tap it shows a brief pressed flash.
* A finger that slides off a held button releases it without firing.
* On a lost phase, a button releases without firing.



##### API

* touch()
* render()

###### 

#### Scroll Bars



Can be tied to other functions, such as:



* Volume
* Monitor brightness, such as on a docked laptop
* etc



Scrollbars can offer styles. Scrollbars cannot stack due to limitations on touch detection. Scrollbars cannot operate vertically.



##### API

* touch()
* render()





#### Data Source



Data sources for visualization, such as cpu usage, memory, storage, etc. This pushes sensor access outside of the main application.



* All data will be reported as arrays of: u32s, u64, i32, i64, f32, f64 depending on data format.
* Plugins will maintain a history of values to recalculate as needed

  * Plugins are advised to consider that visualization maximum width is 640 pixels
* Plugins are responsible for any credentials management or rate limiting required for access to a data source
* Source polling and calls to poll() do not need to be directly connected - plugins may offer cached data
* A source may supply several named series, such as one per CPU core
* Each series describes its data: element type, unit, optional minimum and maximum, and a label
* Series carry no timestamps
* Values in a series run from oldest to newest
* A source's list of series is fixed once it is configured. The host checks series names and counts at startup



##### Configuration and Routing

* Sources are declared by name in a top-level `sources` block, outside any screen. They take no screen space and run all the time.
* A visual names the source it draws from. `source` takes one name or a list. A visual may accept only one, and rejects a longer list when its config is validated.
* `cpu` selects every series from the source `cpu`. `cpu.total` selects only its series `total`. Source names cannot contain dots.
* A `source` that names an undeclared source is a config error. It stops startup, like other config mistakes.
* Several visuals can share a source.
* The host does the routing. It calls poll() on the source at its `update-interval` and hands the data to each visual that names it. Plugins never call each other.
* The first poll follows start() at once, and later ones keep to the interval. If a poll runs past the time of the next, the next comes one interval after it returns. Missed polls are not made up.
* `update-interval` is the time between polls in seconds, from 0.1 to 3600. It defaults to 5. It is a host key, and the host passes it to the source in set_config(), in milliseconds.
* A simple source, such as CPU load, takes one sample per poll(), so its history is evenly spaced.
* A rate-limited source enforces its own limit and returns cached data if polled sooner.
* A source that collects on a separate schedule may add its own key in `config`.
* Visuals have no update setting. They redraw when data arrives.

```yaml
sources:
  cpu:
    plugin: cpu-load
    update-interval: 1
  network:
    plugin: network-throughput
    update-interval: 2
screens:
  main:
    sections:
      - name: cpu-graph
        width: 320
        plugin: histogram
        source: cpu.total
        config:
          style: smooth
```



##### API

* get\_series()
* poll()



#### Data Viz



Visualization engines. Histograms, spectrum analyzers, etc. that can accept data from one or more data sources and are sized by the user.



* Plugins will maintain a buffer that it renders into, and will return a reference to the application when requested.
* Plugins may be stacked as they do not need interaction.
* Plugins are responsible for dealing with datasets smaller than the render window allocated, or whose values don't inherently fit



##### Stacking

Stacking is vertical, within one section (see Screen Space).

```yaml
- name: network
  width: 320
  spacing: just
  stack:
    - plugin: histogram
      source: network.upload
      height: 20
    - plugin: histogram
      source: network.download
      height: 20
```



##### Spacing

When the heights total less than 48 px, `spacing` places the remainder (R = 48 - total). All math is integer, so placement is pixel-exact. Unused rows show the background.

* `just`: the first entry sits at the top and the last at the bottom. R is divided among the gaps between entries. Each gap gets R / gaps, and the R % gaps leftover pixels go one each to the lowest gaps. A single entry has no gaps and is placed as with `margin`.
* `margin`: entries touch each other. The top margin is R / 2 and the bottom margin is R - R / 2, so an odd pixel goes to the bottom.
* `top`: entries touch each other and start at the top, leaving R at the bottom.
* `bottom`: entries touch each other and end at the bottom, leaving R at the top.
* If `spacing` is omitted, it defaults to `margin`.

| Heights | R | `just` gaps, top to bottom | `margin` top / bottom |
| --- | --- | --- | --- |
| 10, 10, 10, 10 | 8 | 2, 3, 3 | 4 / 4 |
| 20, 15 | 13 | 13 | 6 / 7 |



##### API

* render()





### API Interface



#### Plugins



All plugins must export the following interfaces, listed in the order the host calls them.



##### Library

* get\_version()

  * Get plugin version and plugin interface version used
  * App version triple A.B.C
  * Returns two values, the plugin version and the interface version, each as u8\[4], \[0, A, B, C]
  * A change in A or B of the interface version breaks compatibility
  * A change in C may only add fields to the end of structures passed by pointer
  * The plugin version is informational
  * Checks compatibility before any other call
* get\_capabilities()

  * Reports what the library offers: one or more plugins, each with its name and type



##### Plugin

* get\_config()

  * call to get config - typically for defaults on first load



##### Instance

* init()

  * Create an instance of a named plugin
  * Hands the instance the host services, such as text rendering
* set\_config()

  * Validate and apply config, or return a readable error
  * A data source also receives its update-interval
  * A visible instance also receives the width and height of its area
  * A visual also receives the descriptions of the series it draws from, and rejects any it cannot draw
  * The host configures data sources before visuals
* get\_series()

  * Data sources only. Reports the series and their descriptions
  * Called after set\_config()
* start()

  * Tell the instance to begin work
* shutdown()

  * Stop the instance's threads and free it



The following interfaces depend on the type of plugin. They are only called between start() and shutdown().



* poll()

  * Get data from data source. The full active data set is shared: every series, with its description.
* touch(x, phase)

  * Send a touch event to a scroll bar or button
  * Phase is down, move, up or lost
  * lost means the gesture ended without a detected lift: do not activate anything. Actions already taken are not undone
  * x is relative to the section, and may fall outside it if the finger leaves
  * Scroll bar determines how this is interpreted

    * Examples

      * a progress-bar styled scroll bar would move its fill edge to wherever is tapped
      * a classic windows-styled scroll bar would have single-line arrows at the end, and clicking anywhere but the box in the center would scroll a page
* render()

  * Tell a plugin to render itself based on instance, configuration, and data
  * A visual receives the latest data for all of its series on every call. The data is valid only during the call, so a visual copies anything it wants to keep
  * Returns a reference to the instance's buffer: premultiplied BGRA, so the background shows through transparent areas
  * Also returns how long until it wants to be called again, or none
  * The reference is valid only until the next call to that instance. The host copies the buffer on the instance's thread as soon as render() returns, and the main loop uses that copy



##### Host Services

Provided to each instance by init().

* Text rendering
* request\_render()

  * Asks the host to call render(). Can be called from any plugin thread



### Rendering Loop



An instance's render() is called on certain events:

* The instance has just started
* A Data instance has been polled and the data routed to each assigned DataViz
* The instance has received a touch event
* The time returned by its previous render() has passed, such as for the next frame of an animation or the end of a button flash
* The instance has called request\_render()

A render after a touch happens at once. Timed and requested renders come at least 30 ms after the previous render.



The loop will operate as follows:



* Read touch events

  * Decide the gesture's owner (see Touch Handling)
  * If the application owns it, move or switch screens
  * If an instance owns it, send touch events to that instance
* Poll -> Viz

