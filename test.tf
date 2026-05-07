# 1. Define the Provider (Google)
terraform {
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
  }
}

provider "google" {
  project = "project-ec21338a-215e-427c-995"
  region  = "asia-south1"
}

# 2. Create a VPC Network (Best practice for security)
resource "google_compute_network" "vpc_network" {
  name                    = "my-terraform-network"
  auto_create_subnetworks = true
}


resource "google_container_cluster" "primary" {
  name             = var.cluster_name
  location         = var.zone
  network          = google_compute_network.vpc_network.name
  networking_mode  = "VPC_NATIVE"

  remove_default_node_pool = true
  initial_node_count       = 1

  ip_allocation_policy {}

  deletion_protection = false

  logging_config {
    enable_components = []
  }

  monitoring_config {
    enable_components = []
    managed_prometheus {
      enabled = false
    }
  }
}

resource "google_container_node_pool" "primary_nodes" {
  name     = "main-pool"
  location = var.zone
  cluster  = google_container_cluster.primary.name

  autoscaling {
    min_node_count = 1
    max_node_count = 3
  }

  node_config {
    machine_type = "e2-standard-4"
    spot         = true
    disk_type    = "pd-standard"
    disk_size_gb = 30

    oauth_scopes = [
      "https://www.googleapis.com/auth/cloud-platform"
    ]

    labels = {
      environment = "loadtest"
    }
  }
}

resource "google_compute_instance" "k6_runner" {
  name    = "k6-runner-tf"
  machine_type = "n4-standard-4"
  zone = "asia-south1-a"

  boot_disk {
    initialize_params {
      image = "ubuntu-os-cloud/ubuntu-2204-lts"
      size  = 20
    }
  }

  network_interface {
    network =  google_compute_network.vpc_network.name
    access_config {

    }
  }

  service_account {
      scopes = ["cloud-platform"]
    }

    metadata_startup_script = <<-EOF
    #! /bin/bash
    sleep 10

    apt-get update && apt-get install -y wget tar

    wget https://github.com/grafana/k6/releases/download/v0.50.0/k6-v0.50.0-linux-amd64.tar.gz

    tar -xzf k6-v0.50.0-linux-amd64.tar.gz

    sudo mv k6-v0.50.0-linux-amd64/k6 /usr/local/bin/

    rm -rf k6-v0.50.0-linux-amd64*

     EOF


}

resource "google_compute_firewall" "allow_ssh" {
  name    = "allow-ssh-tf"
  network = google_compute_network.vpc_network.name

  allow {
    protocol = "tcp"
    ports    = ["22"]
  }

  source_ranges = ["0.0.0.0/0"] # WARNING:  Good for testing.
}

output "k6_runner_public_ip" {
  value = google_compute_instance.k6_runner.network_interface[0].access_config[0].nat_ip
}

output "kubernetes_cluster_name" {
  value = google_container_cluster.primary.name
}

output "cluster_endpoint" {
  value = google_container_cluster.primary.endpoint
}
