use std::{
    time::Duration,
};
use futures::{
    Future,
    Stream,
    Sink,
    IntoFuture,
    sync::mpsc::{
        self,
        UnboundedSender,
    },
};
use tokio::{
    codec::{
        Framed,
        LinesCodec
    },
    net::{
        TcpListener,
        TcpStream,
    },
};
use tokio_serial::{
    Serial,
    SerialPortSettings,
    DataBits,
    FlowControl,
    Parity,
    StopBits,
};

fn process(stream: TcpStream, uart: UnboundedSender<String>) -> impl Future<Item = (), Error = String> {
    let lines = Framed::new(stream, LinesCodec::new());
    let (_tx, rx) = lines.split();

    rx.map_err(|err| format!("Socket error: {}", err))
        .forward(uart.sink_map_err(|_| String::from("UART is closed")))
        .and_then(|(_tx, _rx)| Ok(()))
        .map_err(|err| format!("Error: {}", err))
}

fn main() {
    let port = {
        let settings = SerialPortSettings {
            baud_rate: 9600,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
            timeout: Duration::from_secs(1),
        };

        Serial::from_path("/dev/ttyUSB0", &settings)
            .expect("Can't open serial port")
    };

    let addr = "0.0.0.0:8080".parse()
        .expect("Can't parse network address");

    let listener = TcpListener::bind(&addr)
        .expect("Can't listen at port");

    let (uart_sender, uart_job) = {
        let (tx, rx) = mpsc::unbounded();
        let lines_port = Framed::new(port, LinesCodec::new());
        let (lines_port, _receiver) = lines_port.split();
        let lines_port = lines_port.sink_map_err(|err| eprintln!("Can't send to serial port: {}", err));
        let job = rx.forward(lines_port)
            .and_then(|(_rx, _port)| Ok(()));
        (tx, job)
    };

    let net_job = listener.incoming()
        .map_err(|err| eprintln!("Can't listen: {}", err))
        .for_each(move |stream| {
            let uart_sender = uart_sender.clone();
            stream.peer_addr().into_future()
                .map_err(|err| eprintln!("Can't get peer address: {}", err))
                .and_then(move |addr| {
                    println!("Connection from {}", addr);
                    tokio::spawn(process(stream, uart_sender)
                        .map_err(|err| eprintln!("{}", err)));
                    Ok(())
                })
        });

    let job = net_job.select(uart_job).then(|_| Ok(()));

    tokio::run(job);
}
