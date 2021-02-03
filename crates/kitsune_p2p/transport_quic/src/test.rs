#[cfg(test)]
mod tests {

    use crate::*;
    use futures::stream::StreamExt;
    use kitsune_p2p_types::transport::*;

    #[tokio::test(threaded_scheduler)]
    async fn test_message() {
        let (listener1, _events1) = spawn_transport_listener_quic(
            ConfigListenerQuic::default().set_override_host(Some("127.0.0.1")),
        )
        .await
        .unwrap();

        let bound1 = listener1.bound_url().await.unwrap();
        assert_eq!("127.0.0.1", bound1.host_str().unwrap());
        println!("listener1 bound to: {}", bound1);

        let (listener2, mut events2) = spawn_transport_listener_quic(ConfigListenerQuic::default())
            .await
            .unwrap();

        metric_task(async move {
            while let Some(evt) = events2.next().await {
                match evt {
                    TransportEvent::IncomingChannel(url, mut write, read) => {
                        println!("events2 incoming connection: {}", url,);
                        let data = read.read_to_end().await;
                        println!("message from {} : {}", url, String::from_utf8_lossy(&data),);
                        let data = format!("echo: {}", String::from_utf8_lossy(&data)).into_bytes();
                        write.write_and_close(data).await?;
                    }
                }
            }
            TransportResult::Ok(())
        });

        let bound2 = listener2.bound_url().await.unwrap();
        println!("listener2 bound to: {}", bound2);

        let resp = listener1.request(bound2, b"hello".to_vec()).await.unwrap();

        println!("got resp: {}", String::from_utf8_lossy(&resp));

        assert_eq!("echo: hello", &String::from_utf8_lossy(&resp));
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_large_message() {
        let (listener1, _events1) = spawn_transport_listener_quic(
            ConfigListenerQuic::default().set_override_host(Some("127.0.0.1")),
        )
        .await
        .unwrap();

        let bound1 = listener1.bound_url().await.unwrap();
        assert_eq!("127.0.0.1", bound1.host_str().unwrap());
        println!("listener1 bound to: {}", bound1);

        let (listener2, mut events2) = spawn_transport_listener_quic(ConfigListenerQuic::default())
            .await
            .unwrap();

        metric_task(async move {
            while let Some(evt) = events2.next().await {
                match evt {
                    TransportEvent::IncomingChannel(_url, mut write, read) => {
                        let data = read.read_to_end().await;
                        let data = format!("echo: {}", String::from_utf8_lossy(&data)).into_bytes();
                        write.write_and_close(data).await?;
                    }
                }
            }
            TransportResult::Ok(())
        });

        let bound2 = listener2.bound_url().await.unwrap();

        let large_msg = std::iter::repeat(b"a"[0]).take(70_000).collect::<Vec<_>>();
        let resp = listener1.request(bound2, large_msg.clone()).await.unwrap();

        assert_eq!(
            format!("echo: {}", String::from_utf8_lossy(&large_msg)),
            String::from_utf8_lossy(&resp)
        );
        assert_eq!(resp.len(), 70_006);
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_max_connections() {
        const NUM_CONNECTIONS: usize = 1;
        const NUM_MESSAGES: usize = 200 * 40;
        let (listener1, mut events1) = spawn_transport_listener_quic(
            ConfigListenerQuic::default().set_override_host(Some("127.0.0.1")),
        )
        .await
        .unwrap();

        let bound1 = listener1.bound_url().await.unwrap();
        assert_eq!("127.0.0.1", bound1.host_str().unwrap());
        println!("listener1 bound to: {}", bound1);

        let mut connections = Vec::new();
        for _ in 0..NUM_CONNECTIONS {
            let con = spawn_transport_listener_quic(ConfigListenerQuic::default())
                .await
                .unwrap();
            connections.push(con);
        }

        let (mut tx, mut rx) = tokio::sync::mpsc::channel(NUM_CONNECTIONS * NUM_MESSAGES);

        metric_task(async move {
            // let mut num = 0;
            while let Some(evt) = events1.next().await {
                // num += 1;
                match evt {
                    TransportEvent::IncomingChannel(_url, write, read) => {
                        // println!("{} events2 incoming connection: {}", num, url,);
                        if let Err(_) = tx.send((write, read)).await {
                            panic!("Failed to send new connection");
                        }
                    }
                }
            }
            TransportResult::Ok(())
        });

        metric_task(async move {
            // tokio::time::delay_for(std::time::Duration::from_secs(20)).await;
            println!("Starting recv");
            // let mut num = 0;
            while let Some((mut write, read)) = rx.recv().await {
                // num += 1;
                let data = read.read_to_end().await;
                // println!("{} message from {}", num, String::from_utf8_lossy(&data),);
                let data = format!("echo: {}", String::from_utf8_lossy(&data)).into_bytes();
                write.write_and_close(data).await?;
            }
            TransportResult::Ok(())
        });

        let mut jhs = Vec::new();
        let mut count = 0;
        for (i, (listener, _)) in connections.into_iter().enumerate() {
            for j in 0..NUM_MESSAGES {
                count += 1;
                if count >= 100 {
                    count = 0;
                    // tokio::time::delay_for(std::time::Duration::from_millis(200)).await;
                }
                let bound1 = bound1.clone();
                let listener = listener.clone();
                let task = async move {
                    let msg = format!("hello from {} msg {}", i, j);
                    // let r = listener.request(bound1, b"hello".to_vec()).await?;
                    let r = listener.request(bound1, msg.into_bytes()).await?;
                    // println!("Got response {}", n + 1);
                    TransportResult::Ok(r)
                };
                jhs.push(metric_task(task));
            }
        }
        // let bound2 = listener2.bound_url().await.unwrap();
        // println!("listener2 bound to: {}", bound2);
        for (i, jh) in jhs.into_iter().enumerate() {
            // println!("Trying to recv {}", i);
            let resp = jh.await.unwrap().unwrap();
            // println!("{} got resp: {}", i, String::from_utf8_lossy(&resp));

            // assert_eq!("echo: hello", &String::from_utf8_lossy(&resp));
        }
    }
}
