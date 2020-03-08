#[cfg(test)]
mod test {
  use crate::{
    connection_like::ConnectionLike, error::Result, from_row, prelude::*, test_misc::get_opts, Conn,
  };
  use mysql_binlog::{event::Event, EventIteratorState};
  use std::io::Cursor;

  async fn gen_dummy_data() -> Result<()> {
    let conn: Conn = Conn::new(get_opts())
      .await?
      .ping()
      .await?
      .drop_query(r"CREATE TABLE IF NOT EXISTS customers (customer_id int not null);")
      .await?;
    let mut conn = conn;
    for i in 0..100 {
      conn = conn
        .drop_query(format!("INSERT INTO customers(customer_id) VALUES({});", i))
        .await?;
    }
    conn.drop_query("DROP TABLE customers;").await?;
    Ok(())
  }

  #[tokio::test]
  async fn should_read_binlog() -> Result<()> {
    let conn: Conn = Conn::new(get_opts()).await?.ping().await?;

    let result = conn.query("SHOW BINARY LOGS;").await?;
    let (conn, (filename, position)) = result
      .reduce_and_drop((String::from("mysqld-bin.000001"), 0), |_, row| {
        let (filename, position): (String, u32) = from_row(row);
        (filename, position)
      })
      .await?;

    println!("BINLOG {} {}", &filename, position);

    gen_dummy_data().await?;

    let conn = conn
      .register_as_slave(12)
      .await?
      .request_binlog(&filename, position, 12)
      .await?;

    let mut state = EventIteratorState::new();

    let mut conn = conn;
    for _ in 0..100 {
      let data = conn.read_packet().await?;
      conn = data.0;
      let data = data.1;

      let mut cursor = Cursor::new(&data);
      // first byte is a NUL byte
      cursor.set_position(1);
      let event = Event::read(&mut cursor, 0).unwrap();
      // println!("{:?}", &event);

      let binlog_event = state.process(event).unwrap();
      if let Some(binlog_event) = binlog_event {
        println!("{:?}", binlog_event);
      }
    }

    Ok(())
  }
}
