# 🎚️🎚️🎚️ Soundwire

`soundwire` is an opinionated audio routing daemon for headless and remote-controlled audio setups.

## 💫 Features
The following features make `soundwire` distinct:

- Made to be run as a system service.
- Dynamic audio device handling with include and exclude filters.
- Playback integration for various protocols - see [Inputs](#inputs).
- Compatible to [Snapcast](https://github.com/badaix/snapcast) remote control protocol.

## 🎤 Sources
`soundwire` currently supports the following source types:
- **pipe**: Creates a unix pipe where sound data is read from
- **device**: Captures sound from a sound input device

The following source types are planned and/or currently in development:
- **pulseaudio**: Creates a pulseaudio TCP server to stream audio using the pulseaudio native protocol
- **librespot**: Spawns a Spotify speaker
- **ROAP**: Create an ROAP/Airplay receiver
- **RTP**: Create an RTP session consumer

## 🔊 Outputs
`soundwire` currently supports playing audio using the following sinks:
- **pipe**: Creates a unix pipe and write sound data to it
- **device**: Playback sound to a sound output device

## 🔧 Configuration
`soundwire` reads a configuration file on startup.
The default config file path is `soundwire.conf` and can be changed using a command line option.
The config file uses the YAML file format and consists of the following sections:
- `outputs`: a list of outputs.
- `sources`: a list of sources.

### `outputs`
Each output entry must consist of the following properties:
- `name`: The name of the output. The name must be unique among all outputs.
- `type`: The type of the output. See [Outputs](#outputs).

All other properties are specific to the output type.

### `sources`
Each source entry must consist of the following properties:
- `name`: The name of the source. The name must be unique among all source.
- `type`: The type of the source. See [Sources](#sources).

All other properties are specific to the source type.

### Example
```yaml
outputs:
    - name: Headphones
      type: device
      device: Name of the sound device
    - name: Screen
      type: device
      device: Name of the sound device
    - name: monitor
      type: pipe
      path: /var/run/soundwire/output

sources:
    - name: mopidy
      type: pipe
      path: /var/run/soundwire/mopidy
    - name: pulseaudio
      type: pipe
      path: /var/run/soundwire/pulseaudio
    - name: legacy
      type: device
      device: Name of the sound device
    - name: aux
      type: device
      device: Name of the sound device
```

## 🤝 Contributing
We welcome contributions from the community to help improve Photonic.
Whether you're a developer, designer, or enthusiast, there are many ways to get involved:

* **Bug Reports:** Report any issues or bugs you encounter while using Photonic.
* **Feature Requests:** Suggest new features or enhancements to make Photonic even more powerful.
* **Pull Requests:** Submit pull requests to address bugs, implement new features, or improve documentation.

## 📄 License
Photonic is licensed under the MIT License, which means you are free to use, modify, and distribute the software for both commercial and non-commercial purposes. See the [LICENSE](./LICENSE) file for more details.

## 🛟 Support
If you have any questions, concerns, or feedback about Photonic, please [contact us](mailto:fooker@lab.sh) or open an issue on the project's GitHub repository.

## 🙏 Acknowledgements
We would like to thank all contributors and supporters who have helped make Photonic possible. Your contributions and feedback are greatly appreciated!

